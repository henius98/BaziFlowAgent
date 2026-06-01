use crate::models::{AppResult, LogErrorExt};
use async_openai::{
    Client,
    config::OpenAIConfig,
    types::chat::{
        ChatCompletionRequestMessage, ChatCompletionTool, ChatCompletionToolChoiceOption, ChatCompletionTools, CreateChatCompletionRequest, CreateChatCompletionRequestArgs, ResponseFormat,
    },
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tracing::{debug, error, info};

/// Configuration for the LLM client (credentials and endpoint)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmClientConfig {
    pub api_key: String,
    pub api_base: String,
    pub timeout_seconds: u64,
    /// Pre-built HTTP client with timeout — reused across all LLM calls
    #[serde(skip)]
    pub http_client: Option<reqwest::Client>,
}

impl LlmClientConfig {
    /// Build and cache the HTTP client. Call once at startup.
    pub fn init_http_client(&mut self) -> Result<(), reqwest::Error> {
        let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(self.timeout_seconds)).build()?;
        self.http_client = Some(client);
        Ok(())
    }
}

/// A parameter model matching the OpenAI chat completion schema.
/// This provides a clean interface for general LLM calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRequestParams {
    // --- Core Configuration ---
    #[serde(skip_serializing)]
    pub model: String,
    pub messages: Vec<ChatCompletionRequestMessage>,

    // --- Creativity & Sampling ---
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,

    // --- Constraints ---
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,

    // --- Penalties ---
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,

    // --- Structured Output & Tooling ---
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ChatCompletionTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ChatCompletionToolChoiceOption>,

    // --- Telemetry & Optimization ---
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_logprobs: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,

    // --- Latest/Advanced Configurations ---
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_completion_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<std::collections::HashMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,

    // --- Custom App Tracking ---
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_type: Option<String>,
}

impl LlmRequestParams {
    pub fn new(model: impl Into<String>, messages: Vec<ChatCompletionRequestMessage>) -> Self {
        Self {
            model: model.into(),
            messages,
            ..Default::default()
        }
    }
}

impl Default for LlmRequestParams {
    fn default() -> Self {
        Self {
            model: "gpt-4o".to_string(), // Can be overridden
            messages: vec![],

            // Core Defaults
            temperature: Some(0.2), // Best practice for analytical tasks
            top_p: Some(1.0),
            seed: None,
            max_tokens: None,
            stop: None,

            // Penalties
            frequency_penalty: None,
            presence_penalty: None,

            // Tooling & Structured Output
            response_format: None,
            tools: None,
            tool_choice: None,

            // Telemetry
            stream: Some(false),
            logprobs: Some(false),
            top_logprobs: None,
            user: None,

            // Advanced / Latest Defaults
            n: None,
            max_completion_tokens: None, // Prevent compatibility issues with non-OpenAI endpoints
            store: None,                 // Some APIs reject this param entirely
            metadata: None,
            reasoning_effort: None,    // Left None because passing this to non-reasoning models (like gpt-4o) causes API errors
            service_tier: None,        // Left None to let OpenAI handle routing
            parallel_tool_calls: None, // Not all models/APIs support this

            // Tracking
            user_id: None,
            request_type: None,
        }
    }
}

/// Build a `CreateChatCompletionRequest` from `LlmRequestParams`.
/// Single source of truth for mapping our params to async-openai's builder.
fn build_chat_request(params: LlmRequestParams) -> AppResult<CreateChatCompletionRequest> {
    let mut builder = CreateChatCompletionRequestArgs::default();
    builder.model(params.model);
    builder.messages(params.messages);

    if let Some(temp) = params.temperature {
        builder.temperature(temp);
    }
    if let Some(top_p) = params.top_p {
        builder.top_p(top_p);
    }
    if let Some(seed) = params.seed {
        builder.seed(seed);
    }
    if let Some(max_tokens) = params.max_tokens {
        builder.max_tokens(max_tokens);
    }
    if let Some(stop) = params.stop {
        builder.stop(stop);
    }
    if let Some(fp) = params.frequency_penalty {
        builder.frequency_penalty(fp);
    }
    if let Some(pp) = params.presence_penalty {
        builder.presence_penalty(pp);
    }
    if let Some(rf) = params.response_format {
        builder.response_format(rf);
    }
    if let Some(tools) = params.tools {
        let tools_enum: Vec<ChatCompletionTools> = tools.into_iter().map(ChatCompletionTools::Function).collect();
        builder.tools(tools_enum);
    }
    if let Some(tc) = params.tool_choice {
        builder.tool_choice(tc);
    }
    if let Some(true) = params.logprobs {
        builder.logprobs(true);
    }
    if let Some(top_logprobs) = params.top_logprobs {
        builder.top_logprobs(top_logprobs);
    }
    if let Some(user) = params.user {
        builder.user(user);
    }
    if let Some(true) = params.stream {
        builder.stream(true);
    }
    if let Some(n) = params.n {
        builder.n(n);
    }
    if let Some(mct) = params.max_completion_tokens {
        builder.max_completion_tokens(mct);
    }
    if let Some(store) = params.store {
        builder.store(store);
    }
    if let Some(metadata) = params.metadata
        && let Ok(meta_value) = serde_json::to_value(metadata)
    {
        builder.metadata(meta_value);
    }
    // Apply Reasoning Effort if provided (now supported in async-openai 0.40)
    if let Some(reasoning) = params.reasoning_effort {
        let effort = match reasoning.to_lowercase().as_str() {
            "low" => async_openai::types::chat::ReasoningEffort::Low,
            "medium" => async_openai::types::chat::ReasoningEffort::Medium,
            "high" => async_openai::types::chat::ReasoningEffort::High,
            _ => async_openai::types::chat::ReasoningEffort::Medium, // Default
        };
        builder.reasoning_effort(effort);
    }
    // Apply Service Tier if provided
    if let Some(tier) = params.service_tier {
        let st = match tier.to_lowercase().as_str() {
            "auto" => async_openai::types::chat::ServiceTier::Auto,
            "default" => async_openai::types::chat::ServiceTier::Default,
            "flex" => async_openai::types::chat::ServiceTier::Flex,
            "scale" => async_openai::types::chat::ServiceTier::Scale,
            "priority" => async_openai::types::chat::ServiceTier::Priority,
            _ => async_openai::types::chat::ServiceTier::Auto, // Default
        };
        builder.service_tier(st);
    }
    if let Some(parallel) = params.parallel_tool_calls {
        builder.parallel_tool_calls(parallel);
    }

    Ok(builder.build()?)
}

/// Provides a general LLM call service using the specified configuration and parameters.
/// Automatically logs every request/response to the `llm_logs` table via fire-and-forget.
/// Depending on `params.stream`, returns either a full response string or a streaming receiver.
pub async fn call_llm(pool: &SqlitePool, config: &LlmClientConfig, params: LlmRequestParams) -> AppResult<crate::models::LlmResponse> {
    let is_stream = params.stream.unwrap_or(false);
    info!("Initializing LLM client for model: {} (streaming: {})", params.model, is_stream);

    // Capture model name and serialized request body before params are consumed by the builder
    let model_for_log = params.model.clone();
    let request_body_json = serde_json::to_string(&params).unwrap_or_default();
    let user_id_log = params.user_id;
    let request_type_log = params.request_type.clone();

    // Reuse the pre-built HTTP client from config (initialized once at startup)
    let http_client = config.http_client.clone().unwrap_or_else(|| {
        tracing::warn!("LLM HTTP client not pre-initialized, building on-the-fly");
        reqwest::Client::builder().timeout(std::time::Duration::from_secs(config.timeout_seconds)).build().unwrap_or_default()
    });

    // Set up OpenAI Client config
    let llm_client = Client::with_config(OpenAIConfig::new().with_api_base(config.api_base.clone()).with_api_key(config.api_key.clone())).with_http_client(http_client);

    let request = build_chat_request(params)?;

    if is_stream {
        debug!("Creating streaming request to LLM...");
        let start = std::time::Instant::now();
        let api_result = llm_client.chat().create_stream(request).await;

        let stream = match api_result {
            Ok(s) => s,
            Err(e) => {
                let pool_clone = pool.clone();
                let err_msg = e.to_string();
                tokio::spawn(async move {
                    let params = crate::repos::LlmLogParams {
                        model: &model_for_log,
                        user_id: user_id_log,
                        request_type: request_type_log.as_deref(),
                        request_body: &request_body_json,
                        response_body: &err_msg,
                        total_tokens: None,
                        duration_ms: start.elapsed().as_millis() as i64,
                        is_success: false,
                    };
                    crate::repos::save_llm_log(&pool_clone, params).await;
                });
                return Err(e).log_err_msg("Failed to create LLM stream");
            }
        };

        let (tx, rx) = tokio::sync::mpsc::channel::<String>(64);
        let pool_clone = pool.clone();

        // Background task: read SSE deltas → forward through channel → log on completion
        tokio::spawn(async move {
            use futures::StreamExt;
            let mut stream = stream;
            let mut full_response = String::new();

            while let Some(result) = stream.next().await {
                match result {
                    Ok(response) => {
                        for choice in &response.choices {
                            if let Some(content) = &choice.delta.content {
                                full_response.push_str(content);
                                if tx.send(content.clone()).await.is_err() {
                                    // Receiver dropped — stop reading
                                    return;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("LLM stream error: {}", e);
                        break;
                    }
                }
            }

            let duration_ms = start.elapsed().as_millis() as i64;
            // Fire-and-forget: log the accumulated response
            let params = crate::repos::LlmLogParams {
                model: &model_for_log,
                user_id: user_id_log,
                request_type: request_type_log.as_deref(),
                request_body: &request_body_json,
                response_body: &full_response,
                total_tokens: None,
                duration_ms,
                is_success: !full_response.is_empty(),
            };
            crate::repos::save_llm_log(&pool_clone, params).await;
        });

        Ok(crate::models::LlmResponse::Stream(rx))
    } else {
        debug!("Sending general request to LLM...");
        let start = std::time::Instant::now();
        let api_result = llm_client.chat().create(request).await;
        let duration_ms = start.elapsed().as_millis() as i64;

        // Fire-and-forget: log the call to the database without blocking the response path
        let pool_clone = pool.clone();
        match &api_result {
            Ok(response) => {
                let total_tokens = response.usage.as_ref().map(|u| u.total_tokens as i64);
                let response_json = serde_json::to_string(response).unwrap_or_default();
                tokio::spawn(async move {
                    let params = crate::repos::LlmLogParams {
                        model: &model_for_log,
                        user_id: user_id_log,
                        request_type: request_type_log.as_deref(),
                        request_body: &request_body_json,
                        response_body: &response_json,
                        total_tokens,
                        duration_ms,
                        is_success: true,
                    };
                    crate::repos::save_llm_log(&pool_clone, params).await;
                });
            }
            Err(e) => {
                let err_msg = e.to_string();
                tokio::spawn(async move {
                    let params = crate::repos::LlmLogParams {
                        model: &model_for_log,
                        user_id: user_id_log,
                        request_type: request_type_log.as_deref(),
                        request_body: &request_body_json,
                        response_body: &err_msg,
                        total_tokens: None,
                        duration_ms,
                        is_success: false,
                    };
                    crate::repos::save_llm_log(&pool_clone, params).await;
                });
            }
        }

        let response = api_result.log_err_msg("General LLM call failed")?;
        debug!("Received response from general LLM service {:?}", response);

        let content = response.choices.first().and_then(|c| c.message.content.clone()).unwrap_or_default();
        Ok(crate::models::LlmResponse::Full(content))
    }
}
