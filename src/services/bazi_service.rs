use chrono::Datelike;
use tracing::error;

use crate::repos;
use crate::services::{paipan, solar_time};

pub struct BaziDataParams<'a> {
    pub user_id: u64,
    pub username: &'a str,
    pub birth_date: &'a str,
    pub birth_hour: u8,
    pub birth_minute: u8,
    pub gender: u8,
    pub location: Option<String>,
}

pub async fn prepare_bazi_data(state: &std::sync::Arc<crate::models::AppState>, params: BaziDataParams<'_>) -> crate::models::AppResult<paipan::StructuredBazi> {
    // Calculate True Solar Time if location is provided
    let naive_dt = match chrono::NaiveDate::parse_from_str(params.birth_date, "%Y-%m-%d") {
        Ok(d) => match d.and_hms_opt(params.birth_hour as u32, params.birth_minute as u32, 0) {
            Some(dt) => dt,
            None => {
                error!("Invalid hour/minute: {}:{}", params.birth_hour, params.birth_minute);
                return Err(crate::models::error::AppError::Message("Invalid time".to_string()));
            }
        },
        Err(e) => {
            error!("Invalid date format {}: {}", params.birth_date, e);
            return Err(crate::models::error::AppError::Message("Invalid date".to_string()));
        }
    };

    let solar_dt = if let Some(city_name) = &params.location {
        solar_time::calculate_true_solar_time(naive_dt, city_name, 120.0)
    } else {
        naive_dt
    };

    let birth_year = naive_dt.year();
    let (structured_data, structured_json) = paipan::fetch_bazi_chart(&state.http_client, solar_dt, params.gender, birth_year, params.location).await?;

    repos::upsert_user_bazi(&state.db_pool, params.user_id, Some(params.username), &structured_json, params.gender, params.birth_date).await;

    Ok(structured_data)
}

pub async fn build_and_save_bazi_html(user_id: u64, username: &str, structured_data: &paipan::StructuredBazi) {
    let html_diagram = paipan::generate_bazi_html(structured_data, username);
    let filename = format!("bazi_{}.html", user_id);
    let public_path = std::path::PathBuf::from("public").join(&filename);
    if let Err(e) = tokio::fs::write(&public_path, html_diagram).await {
        error!("Failed to save Bazi HTML to public: {}", e);
    }
}

/// Core logic for Bazi chart calculation and destiny reading generation.
pub async fn core_bazi_analysis(
    state: &std::sync::Arc<crate::models::AppState>,
    structured_data: &paipan::StructuredBazi,
    llm_model: Option<crate::models::common::LlmModel>,
) -> crate::models::AppResult<tokio::sync::mpsc::Receiver<String>> {
    let system_prompt = include_str!("../../prompts/UserBaziAssistant.md");
    let full_user_content = format!("【待分析命盘】 [Bazi Info]\n{}", structured_data);
    let system_message = match async_openai::types::chat::ChatCompletionRequestSystemMessageArgs::default().content(system_prompt).build() {
        Ok(m) => m,
        Err(_) => return Err(crate::models::error::AppError::Message("Failed to build system message".to_string())),
    };
    let user_message = match async_openai::types::chat::ChatCompletionRequestUserMessageArgs::default().content(full_user_content).build() {
        Ok(m) => m,
        Err(_) => return Err(crate::models::error::AppError::Message("Failed to build user message".to_string())),
    };

    let model_name = llm_model.map(|m| m.as_str().to_string()).unwrap_or_else(|| state.config.llm_model_name.clone());
    let mut params = crate::services::llm::LlmRequestParams::new(model_name, vec![system_message.into(), user_message.into()]);
    params.frequency_penalty = Some(0.5);
    params.presence_penalty = Some(0.5);
    params.temperature = Some(0.2);
    params.top_p = Some(0.75);
    params.stream = Some(true);

    match crate::services::llm::call_llm(&state.db_pool, &state.config.llm_client_config, params).await {
        Ok(crate::models::LlmResponse::Stream(receiver)) => Ok(receiver),
        Ok(_) => Err(crate::models::error::AppError::Message("Expected stream response from LLM".into())),
        Err(e) => Err(e),
    }
}
