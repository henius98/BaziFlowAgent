use crate::models::{AppError, AppResult, LogErrorExt};
use async_openai::{
    Client,
    config::OpenAIConfig,
    types::{chat::ChatCompletionRequestSystemMessageArgs, chat::ChatCompletionRequestUserMessageArgs, chat::CreateChatCompletionRequestArgs},
};
use reqwest::Client as HttpClient;
use tracing::{debug, info};

pub struct BaziReadingRequest<'a> {
    pub http_client: &'a HttpClient,
    pub date_value: &'a str,
    pub history_msg: &'a str,
    pub user_bazi_four_pillars: &'a str,
    pub destiny_reading: &'a str,
    pub api_key: &'a str,
    pub api_base: &'a str,
    pub model_name: &'a str,
}

pub async fn generate_bazi_reading(req: BaziReadingRequest<'_>) -> AppResult<String> {
    info!("Fetching almanac data for {}", req.date_value);

    // 1. Fetch and format almanac data
    let almanac_data = crate::services::almanac::fetch_and_format_almanac(req.http_client, req.date_value)
        .await
        .log_err_msg("Failed to fetch or format almanac data")?;

    info!("Almanac data fetched successfully. Building LLM prompt...");

    // 2. Set up OpenAI Client
    let mut config = OpenAIConfig::new().with_api_key(req.api_key);
    if !req.api_base.is_empty() {
        config = config.with_api_base(req.api_base);
    }
    let llm_client = Client::with_config(config);

    // 3. Build Prompt
    // Load the system prompt from the markdown file
    let system_prompt_template = include_str!("../../prompts/BaziHuangLiAssistant.md");

    let system_message = ChatCompletionRequestSystemMessageArgs::default().content(system_prompt_template).build()?;

    let context_data = if req.history_msg.is_empty() {
        almanac_data.clone()
    } else {
        format!("{}\n\n{}", almanac_data, req.history_msg)
    };

    let user_content = if req.destiny_reading.is_empty() {
        format!(
            "请结合下信息以便进行精确排盘与推演：\n{}\n预测目标日期:{}\n{}",
            req.user_bazi_four_pillars, req.date_value, context_data
        )
    } else {
        format!(
            "请结合以下信息进行精确的日运势推演：\n\n【用户八字排盘】\n{}\n\n【用户命格详批】\n{}\n\n【目标预测日期】\n{}\n\n【其他背景信息】\n{}",
            req.user_bazi_four_pillars, req.destiny_reading, req.date_value, context_data
        )
    };

    debug!("Full System Prompt:\n{}", system_prompt_template);
    debug!("Full User Prompt:\n{}", user_content);

    let user_message = ChatCompletionRequestUserMessageArgs::default().content(user_content).build()?;

    // 4. Request
    let request = CreateChatCompletionRequestArgs::default()
        .model(req.model_name)
        .messages([system_message.into(), user_message.into()])
        .frequency_penalty(0.5)
        .presence_penalty(0.5)
        .temperature(0.2)
        .top_p(0.75)
        .build()?;

    info!("Sending request to LLM (Model: {})...", req.model_name);
    let response = llm_client.chat().create(request).await.log_err_msg("LLM call failed")?;

    if let Some(content) = response.choices.first().and_then(|c| c.message.content.as_ref()) {
        info!("Received response from LLM");
        return Ok(content.clone());
    }

    Err(AppError::context("No valid content in LLM response"))
}

pub async fn generate_destiny_reading(user_bazi_four_pillars_text: &str, api_key: &str, api_base: &str, model_name: &str) -> AppResult<String> {
    info!("Generating destiny reading for new bazi profile...");

    // Set up OpenAI Client
    let mut config = OpenAIConfig::new().with_api_key(api_key);
    if !api_base.is_empty() {
        config = config.with_api_base(api_base);
    }
    let llm_client = Client::with_config(config);

    // System prompt is the static instruction set
    let system_prompt = include_str!("../../prompts/UserBazi.md");

    debug!("Full System Prompt:\n{}", system_prompt);
    let full_user_content = format!("【待分析命盘】 [Bazi Info]\n{}", user_bazi_four_pillars_text);
    debug!("Full User Prompt:\n{}", full_user_content);

    let system_message = ChatCompletionRequestSystemMessageArgs::default().content(system_prompt).build()?;
    let user_message = ChatCompletionRequestUserMessageArgs::default().content(full_user_content).build()?;

    let request = CreateChatCompletionRequestArgs::default()
        .model(model_name)
        .messages([system_message.into(), user_message.into()])
        .frequency_penalty(0.5)
        .presence_penalty(0.5)
        .temperature(0.2)
        .top_p(0.75)
        .build()?;

    info!("Sending destiny reading request to LLM (Model: {})...", model_name);
    let response = llm_client.chat().create(request).await.log_err_msg("LLM call failed")?;

    if let Some(content) = response.choices.first().and_then(|c| c.message.content.as_ref()) {
        info!("Received destiny reading response from LLM");
        return Ok(content.clone());
    }

    Err(AppError::context("No valid content in LLM response"))
}
