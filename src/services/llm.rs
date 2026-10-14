use crate::models::{AppResult, LlmRequestType, LogErrorExt};
use async_openai::{
  Client,
  config::OpenAIConfig,
  types::chat::{ChatCompletionRequestMessage, ChatCompletionStreamOptions, CreateChatCompletionRequest, CreateChatCompletionRequestArgs, ReasoningEffort},
};
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info};

/// Configuration for the LLM client (credentials and endpoint)
#[derive(Clone, Serialize, Deserialize)]
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

/// A parameter model focused on application domain needs.
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
  pub max_tokens: Option<u32>,

  // --- Telemetry & Optimization ---
  #[serde(skip_serializing_if = "Option::is_none")]
  pub stream: Option<bool>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub reasoning_effort: Option<ReasoningEffort>,

  // --- Custom App Tracking ---
  #[serde(skip_serializing_if = "Option::is_none")]
  pub user_id: Option<i64>,
  #[serde(skip_serializing)]
  pub request_type: LlmRequestType,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub model_id: Option<i64>,
}

impl LlmRequestParams {
  pub fn new(model: impl Into<String>, messages: Vec<ChatCompletionRequestMessage>, request_type: LlmRequestType) -> Self {
    Self {
      model: model.into(), // Required per-call model selection
      messages,

      // Core Defaults
      temperature: Some(0.2), // Best practice for analytical tasks
      top_p: Some(1.0),
      max_tokens: None,

      // Telemetry
      stream: Some(false),
      reasoning_effort: None,

      // Tracking
      user_id: None,
      request_type,
      model_id: None,
    }
  }

  /// Preset for analytical readings (low temperature, nucleus sampling)
  pub fn analytical(model: impl Into<String>, messages: Vec<ChatCompletionRequestMessage>, request_type: LlmRequestType) -> Self {
    Self { temperature: Some(0.2), top_p: Some(0.75), ..Self::new(model, messages, request_type) }
  }

  /// Preset for conversational turns (balanced temperature)
  pub fn chat(model: impl Into<String>, messages: Vec<ChatCompletionRequestMessage>, request_type: LlmRequestType) -> Self {
    Self { temperature: Some(0.4), top_p: Some(1.0), ..Self::new(model, messages, request_type) }
  }

  /// Preset for deterministic compact summaries
  pub fn deterministic(model: impl Into<String>, messages: Vec<ChatCompletionRequestMessage>, request_type: LlmRequestType) -> Self {
    Self { temperature: Some(0.1), top_p: Some(0.75), ..Self::new(model, messages, request_type) }
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
  if let Some(max_tokens) = params.max_tokens {
    builder.max_tokens(max_tokens);
  }
  if let Some(true) = params.stream {
    builder.stream(true);
    builder.stream_options(ChatCompletionStreamOptions { include_usage: Some(true), include_obfuscation: None });
  }
  if let Some(effort) = params.reasoning_effort {
    builder.reasoning_effort(effort);
  }

  Ok(builder.build()?)
}

use futures::future::BoxFuture;

/// Abstract service contract for Large Language Model completions.
pub trait LlmService: Send + Sync + std::fmt::Debug {
  fn call<'a>(&'a self, pool: &'a crate::repos::Database, params: LlmRequestParams, runtime: &'a crate::models::runtime::Runtime) -> BoxFuture<'a, AppResult<crate::models::LlmResponse>>;
}

/// Default production LLM service implementation.
#[derive(Debug, Clone)]
pub struct DefaultLlmService {
  pub config: LlmClientConfig,
}

impl DefaultLlmService {
  pub fn new(config: LlmClientConfig) -> Self {
    Self { config }
  }
}

impl LlmService for DefaultLlmService {
  fn call<'a>(&'a self, pool: &'a crate::repos::Database, params: LlmRequestParams, runtime: &'a crate::models::runtime::Runtime) -> BoxFuture<'a, AppResult<crate::models::LlmResponse>> {
    Box::pin(call_llm(pool, &self.config, params, runtime))
  }
}

/// Provides a general LLM call service using the specified configuration and parameters.
/// Logs the full request and response, including provider errors, for each LLM call.
/// Depending on `params.stream`, returns either a full response string or a streaming receiver.
pub async fn call_llm(pool: &crate::repos::Database, config: &LlmClientConfig, params: LlmRequestParams, runtime: &crate::models::runtime::Runtime) -> AppResult<crate::models::LlmResponse> {
  let permit = runtime.llm.clone().try_acquire_owned().map_err(|_| anyhow::anyhow!("LLM capacity exhausted"))?;
  let is_stream = params.stream.unwrap_or(false);
  info!("Initializing LLM client for model: {} (streaming: {})", params.model, is_stream);

  // Capture tracking fields before params are consumed by the request builder.
  let user_id_log = params.user_id;
  let request_type_log = params.request_type;
  let model_id_log = params.model_id;
  let reasoning_effort_log = params.reasoning_effort.as_ref().map(|effort| match effort {
    ReasoningEffort::None => "none",
    ReasoningEffort::Minimal => "minimal",
    ReasoningEffort::Low => "low",
    ReasoningEffort::Medium => "medium",
    ReasoningEffort::High => "high",
    ReasoningEffort::Xhigh => "xhigh",
    ReasoningEffort::Max => "max",
  });

  // Reuse the pre-built HTTP client from config (initialized once at startup)
  let http_client = match config.http_client.clone() {
    Some(client) => client,
    None => {
      tracing::warn!("LLM HTTP client not pre-initialized, building on-the-fly");
      reqwest::Client::builder().timeout(std::time::Duration::from_secs(config.timeout_seconds)).build().log_err_msg("Failed to build fallback HTTP client")?
    }
  };

  // Set up OpenAI Client config
  let llm_client = Client::with_config(OpenAIConfig::new().with_api_base(config.api_base.clone()).with_api_key(config.api_key.clone())).with_http_client(http_client);

  let request = build_chat_request(params)?;
  let request_body_json = serde_json::to_string(&request)?;

  if is_stream {
    debug!("Creating streaming request to LLM...");
    let start = std::time::Instant::now();
    let api_result = llm_client.chat().create_stream(request).await;

    let stream = match api_result {
      Ok(s) => s,
      Err(e) => {
        let err_msg = error_response_body(&e);
        let params = crate::repos::LlmLogParams {
          user_id: user_id_log,
          request_type: request_type_log.as_str(),
          request_body: &request_body_json,
          response_body: &err_msg,
          total_tokens: None,
          duration_ms: start.elapsed().as_millis() as i64,
          is_success: false,
          model_id: model_id_log,
          reasoning_effort: reasoning_effort_log,
        };
        crate::repos::save_llm_log(pool, params).await;
        return Err(e).log_err_msg("Failed to create LLM stream");
      }
    };

    let (tx, rx) = tokio::sync::mpsc::channel::<tokio_util::bytes::Bytes>(64);
    let (completion, completed) = tokio::sync::oneshot::channel();
    let pool_clone = pool.clone();

    // Background task: read SSE deltas → forward through channel → log on completion
    let shutdown = runtime.shutdown.clone();
    runtime.tasks.spawn(async move {
      let _permit = permit;
      use futures::StreamExt;
      let mut stream = stream;
      let mut response_bytes = 0usize;
      let mut response_text = String::new();
      let mut stream_error = None;
      let mut total_tokens = None;

      'stream_loop: loop {
        let result = tokio::select! {
          _ = shutdown.cancelled() => { stream_error = Some("LLM stream cancelled during shutdown".to_owned()); break 'stream_loop; },
          _ = tx.closed() => { stream_error = Some("LLM stream receiver closed".to_owned()); break 'stream_loop; },
          result = stream.next() => match result { Some(result) => result, None => break },
        };
        match result {
          Ok(response) => {
            total_tokens = response.usage.as_ref().map(|usage| usage.total_tokens as i64).or(total_tokens);
            for choice in response.choices {
              if let Some(content) = choice.delta.content {
                response_bytes = response_bytes.saturating_add(content.len());
                if response_bytes > MAX_RESPONSE_BYTES {
                  tracing::warn!("LLM response byte limit exceeded");
                  stream_error = Some("LLM response byte limit exceeded".to_owned());
                  break 'stream_loop;
                }
                response_text.push_str(&content);
                let chunk_bytes = tokio_util::bytes::Bytes::from(content);
                if !matches!(tokio::time::timeout(std::time::Duration::from_secs(5), tx.send(chunk_bytes)).await, Ok(Ok(()))) {
                  // Receiver dropped — stop reading
                  stream_error = Some("LLM stream receiver unavailable".to_owned());
                  break 'stream_loop;
                }
              }
            }
          }
          Err(e) => {
            error!("LLM stream failed");
            stream_error = Some(e.to_string());
            break;
          }
        }
      }

      let succeeded = stream_error.is_none() && response_bytes > 0;
      if !succeeded && stream_error.is_none() {
        stream_error = Some("LLM stream returned no content".to_owned());
      }
      drop(tx);
      let duration_ms = start.elapsed().as_millis() as i64;
      // Keep the complete text and any error after the stream has ended.
      let response_body = serde_json::json!({ "content": response_text, "error": stream_error }).to_string();
      let params = crate::repos::LlmLogParams {
        user_id: user_id_log,
        request_type: request_type_log.as_str(),
        request_body: &request_body_json,
        response_body: &response_body,
        total_tokens,
        duration_ms,
        is_success: succeeded,
        model_id: model_id_log,
        reasoning_effort: reasoning_effort_log,
      };
      crate::repos::save_llm_log(&pool_clone, params).await;
      let _ = completion.send(succeeded);
    });

    Ok(crate::models::LlmResponse::Stream(crate::models::LlmStream::new(rx, completed)))
  } else {
    debug!("Sending general request to LLM...");
    let start = std::time::Instant::now();
    let api_result = llm_client.chat().create(request).await;
    let duration_ms = start.elapsed().as_millis() as i64;

    let (response_body, total_tokens, is_success) = match &api_result {
      Ok(response) => {
        let total_tokens = response.usage.as_ref().map(|u| u.total_tokens as i64);
        let content_too_large = response.choices.iter().any(|choice| choice.message.content.as_ref().is_some_and(|content| content.len() > MAX_RESPONSE_BYTES));
        if content_too_large { (error_response_body(&"LLM response byte limit exceeded"), total_tokens, false) } else { (serde_json::to_string(response)?, total_tokens, true) }
      }
      Err(e) => (error_response_body(e), None, false),
    };
    let params = crate::repos::LlmLogParams {
      user_id: user_id_log,
      request_type: request_type_log.as_str(),
      request_body: &request_body_json,
      response_body: &response_body,
      total_tokens,
      duration_ms,
      is_success,
      model_id: model_id_log,
      reasoning_effort: reasoning_effort_log,
    };
    crate::repos::save_llm_log(pool, params).await;

    let response = api_result.log_err_msg("General LLM call failed")?;
    debug!("Received response from general LLM service");

    let content = response.choices.into_iter().next().and_then(|c| c.message.content).unwrap_or_default();
    if content.len() > MAX_RESPONSE_BYTES {
      return Err(anyhow::anyhow!("LLM response byte limit exceeded").into());
    }
    Ok(crate::models::LlmResponse::Full(content))
  }
}

/// Upper bound for accumulated analysis and streaming output.
pub const MAX_RESPONSE_BYTES: usize = 1_048_576;

fn error_response_body(error: &impl std::fmt::Display) -> String {
  serde_json::json!({ "error": error.to_string() }).to_string()
}

impl std::fmt::Debug for LlmClientConfig {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("LlmClientConfig").field("timeout_seconds", &self.timeout_seconds).finish_non_exhaustive()
  }
}
