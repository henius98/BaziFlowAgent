use crate::logger::{AppResult, LogErrorExt};
use async_openai::{
    Client,
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestUserMessageArgs,
        CreateChatCompletionRequestArgs,
    },
};
use reqwest::Client as HttpClient;
use tracing::info;

pub async fn generate_bazi_reading(
    http_client: &HttpClient,
    date_value: &str,
    history_msg: &str,
    user_bazi: &str,
    api_key: &str,
    api_base: &str,
    model_name: &str,
) -> AppResult<String> {
    info!("Fetching almanac data for {}", date_value);

    // 1. Fetch and format almanac data
    let almanac_data = crate::almanac::fetch_and_format_almanac(http_client, date_value)
        .await
        .log_err_msg("Failed to fetch or format almanac data")?;

    info!("Almanac data fetched successfully. Building LLM prompt...");

    // 2. Set up OpenAI Client
    let mut config = OpenAIConfig::new().with_api_key(api_key);
    if !api_base.is_empty() {
        config = config.with_api_base(api_base);
    }
    let llm_client = Client::with_config(config);

    // 3. Build Prompt
    // Load the system prompt from the markdown file
    let system_prompt_template = include_str!("../BaziHuangLiAssistantPrompt.md");

    let system_message = ChatCompletionRequestSystemMessageArgs::default()
        .content(system_prompt_template)
        .build()?;

    let context_data = if history_msg.is_empty() {
        almanac_data.clone()
    } else {
        format!("{}\n\n{}", almanac_data, history_msg)
    };

    let user_content = format!(
        "请结合下信息以便进行精确排盘与推演：\n{}\n预测目标日期：{}\n{}",
        user_bazi, date_value, context_data
    );

    let user_message = ChatCompletionRequestUserMessageArgs::default()
        .content(user_content)
        .build()?;

    // 4. Request
    let request = CreateChatCompletionRequestArgs::default()
        .model(model_name)
        .messages([system_message.into(), user_message.into()])
        .frequency_penalty(0.5)
        .presence_penalty(0.5)
        .temperature(0.2)
        .top_p(0.75)
        .build()?;

    info!("Sending request to LLM (Model: {})...", model_name);
    let response = llm_client
        .chat()
        .create(request)
        .await
        .log_err_msg("LLM call failed")?;

    if let Some(choice) = response.choices.first() {
        if let Some(ref content) = choice.message.content {
            info!("Received response from LLM");
            return Ok(content.clone());
        }
    }

    Err(crate::logger::AppError::context("No valid content in LLM response"))
}
