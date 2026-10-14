use crate::services::paipan;
use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime};
use tracing::error;

pub struct DestinyProfileParams<'a> {
  pub user_id: u64,
  pub username: &'a str,
  pub birth_date: &'a str,
  pub birth_time: Option<NaiveTime>,
  pub gender: u8,
  pub location: Option<String>,
  pub llm_model: Option<crate::models::common::LlmModel>,
}

pub struct DestinyProfileResult {
  pub chart_url: String,
  pub stream: crate::models::LlmStream,
}

const UNKNOWN_BIRTH_TIME_REFERENCE_HOUR: u32 = 12;

fn resolve_birth_datetime(birth_date: NaiveDate, birth_time: Option<NaiveTime>) -> crate::models::AppResult<(NaiveDateTime, bool)> {
  let birth_time_known = birth_time.is_some();
  let time = match birth_time {
    Some(time) => time,
    None => NaiveTime::from_hms_opt(UNKNOWN_BIRTH_TIME_REFERENCE_HOUR, 0, 0).ok_or_else(|| crate::models::error::AppError::Message("Failed to construct the unknown-time reference".to_string()))?,
  };

  Ok((birth_date.and_time(time), birth_time_known))
}

// Note on birth time resolution:
// A noon reference keeps an unknown time away from a date boundary. It is
// metadata-marked below so consumers do not treat its hour pillar as certain.
// Calculate True Solar Time only when the location is known.

pub async fn build_and_save_bazi_html(state: &std::sync::Arc<crate::models::AppState>, user_id: u64, username: &str, structured_data: &paipan::StructuredBazi) {
  state.events.publish(user_id, "profile.updated", "application");
  let html_diagram = paipan::generate_bazi_html(structured_data, username);
  let filename = format!("bazi_{}.html", user_id);

  if let Err(e) = state.blob_store.put(&filename, html_diagram.as_bytes(), Some("text/html")).await {
    error!("Failed to upload Bazi HTML: {}", e);
  }
}

pub async fn get_bazi_chart_url(state: &std::sync::Arc<crate::models::AppState>, user_id: u64) -> crate::models::AppResult<String> {
  let filename = format!("bazi_{}.html", user_id);
  // Presign URL valid for 24 hours (86400 seconds)
  state.blob_store.presign_get(&filename, 86400).await
}

/// Single entry point for creating or replacing a user's Destiny Profile.
///
/// Orchestrates solar time calculation, chart fetching, database persistence,
/// session context invalidation, HTML rendering, capability URL generation,
/// and streaming analysis with automatic persistence and background summarization.
pub async fn create_destiny_profile(state: &std::sync::Arc<crate::models::AppState>, params: DestinyProfileParams<'_>) -> crate::models::AppResult<DestinyProfileResult> {
  let birth_date = match chrono::NaiveDate::parse_from_str(params.birth_date, "%Y-%m-%d") {
    Ok(date) => date,
    Err(e) => {
      error!("Invalid date format {}: {}", params.birth_date, e);
      return Err(crate::models::error::AppError::Message("Invalid date".to_string()));
    }
  };

  // A noon reference keeps an unknown time away from a date boundary. It is
  // metadata-marked below so consumers do not treat its hour pillar as certain.
  let (naive_dt, birth_time_known) = resolve_birth_datetime(birth_date, params.birth_time)?;

  // Calculate True Solar Time internally when the location is known.
  let solar_dt = crate::services::solar_time::to_true_solar_time(naive_dt, params.location.as_deref());

  let birth_year = naive_dt.year();
  let (structured_data, structured_json) = state.chart_provider.fetch_chart(solar_dt, params.gender, birth_year, params.location, birth_time_known).await?;

  crate::repos::upsert_user_bazi(&state.db_pool, params.user_id, Some(params.username), &structured_json, params.gender, params.birth_date).await?;

  // Invalidate old chat turns derived from the previous profile.
  {
    let mut ctx = state.user_contexts.entry(params.user_id).or_default();
    ctx.clear_messages();
    ctx.history_loaded = true;
    ctx.last_active = chrono::Utc::now();
  }
  if let Err(e) = state.chat_cache.clear(params.user_id).await {
    error!("Failed to clear stale chat history for user {}: {}", params.user_id, e);
  }

  build_and_save_bazi_html(state, params.user_id, params.username, &structured_data).await;

  let chart_url = match get_bazi_chart_url(state, params.user_id).await {
    Ok(url) => url,
    Err(e) => {
      error!("Failed to generate bazi chart URL: {}", e);
      format!("{}/bazi_{}.html", state.config.base_url.trim_end_matches('/'), params.user_id)
    }
  };

  let raw_stream = core_bazi_analysis(state, params.user_id, &structured_data, params.llm_model.clone()).await?;

  let state_clone = state.clone();
  let user_id = params.user_id;
  let structured_bazi = structured_data.to_string();
  let llm_model = params.llm_model;

  let stream = crate::services::conversation::wrap_persisted_stream(&state.runtime.tasks, raw_stream, move |text| async move {
    crate::repos::save_user_bazi_analysis(&state_clone.db_pool, user_id, &text).await;
    crate::services::conversation::persist_turn_response(&state_clone, user_id, None, &text).await;
    if let Ok(summary) = generate_bazi_summary(&state_clone, user_id, &structured_bazi, &text, llm_model).await {
      crate::repos::save_user_bazi_summary(&state_clone.db_pool, user_id, &summary).await;
    } else {
      tracing::error!("Failed to generate bazi summary for user {}", user_id);
    }
  });

  Ok(DestinyProfileResult { chart_url, stream })
}

/// Core logic for Bazi chart calculation and destiny reading generation.
pub async fn core_bazi_analysis(
  state: &std::sync::Arc<crate::models::AppState>,
  user_id: u64,
  structured_data: &paipan::StructuredBazi,
  llm_model: Option<crate::models::common::LlmModel>,
) -> crate::models::AppResult<crate::models::LlmStream> {
  let system_prompt = include_str!("../../prompts/UserBaziAssistant.md");
  let full_user_content = format!("【待分析命盘】 [Bazi Info]\n{}", structured_data);
  let system_message = async_openai::types::chat::ChatCompletionRequestSystemMessageArgs::default().content(system_prompt).build()?;
  let user_message = async_openai::types::chat::ChatCompletionRequestUserMessageArgs::default().content(full_user_content).build()?;

  let model = crate::repos::resolve_llm_model(&state.db_pool, llm_model).await?;
  let model_id = Some(model.id() as i64);
  let model_name = model.as_str().to_string();
  let params = crate::services::llm::LlmRequestParams {
    temperature: Some(0.2),
    top_p: Some(0.75),
    stream: Some(true),
    user_id: Some(user_id as i64),
    model_id,
    ..crate::services::llm::LlmRequestParams::new(model_name, vec![system_message.into(), user_message.into()], crate::models::LlmRequestType::BaziAnalysis)
  };

  match state.llm_service.call(&state.db_pool, params, &state.runtime).await {
    Ok(crate::models::LlmResponse::Stream(receiver)) => Ok(receiver),
    Ok(_) => Err(crate::models::error::AppError::Message("Expected stream response from LLM".into())),
    Err(e) => Err(e),
  }
}

pub async fn generate_bazi_summary(
  state: &std::sync::Arc<crate::models::AppState>,
  user_id: u64,
  structured_bazi: &str,
  bazi_analysis: &str,
  llm_model: Option<crate::models::common::LlmModel>,
) -> crate::models::AppResult<String> {
  let system_prompt = include_str!("../../prompts/BaziSummaryAssistant.md");
  let full_user_content = format!("【应用提供的结构化命盘事实】\n{}\n【用户命盘详批（既有派生解释）】\n{}", structured_bazi, bazi_analysis);

  let system_message = async_openai::types::chat::ChatCompletionRequestSystemMessageArgs::default().content(system_prompt).build()?;
  let user_message = async_openai::types::chat::ChatCompletionRequestUserMessageArgs::default().content(full_user_content).build()?;

  let model = crate::repos::resolve_llm_model(&state.db_pool, llm_model).await?;
  let model_id = Some(model.id() as i64);
  let model_name = model.as_str().to_string();
  let params = crate::services::llm::LlmRequestParams {
    temperature: Some(0.1),
    top_p: Some(0.7),
    stream: Some(false),
    user_id: Some(user_id as i64),
    model_id,
    ..crate::services::llm::LlmRequestParams::new(model_name, vec![system_message.into(), user_message.into()], crate::models::LlmRequestType::GenerateBaziSummary)
  };

  match state.llm_service.call(&state.db_pool, params, &state.runtime).await {
    Ok(crate::models::LlmResponse::Full(summary)) => validate_bazi_summary(&summary),
    Ok(_) => Err(crate::models::error::AppError::Message("Expected full response from LLM".into())),
    Err(e) => Err(e),
  }
}

fn validate_bazi_summary(summary: &str) -> crate::models::AppResult<String> {
  let summary = summary.trim();
  if summary.is_empty() {
    return Err(crate::models::error::AppError::Message("Generated Bazi summary was empty".to_string()));
  }
  if summary.chars().count() > 200 {
    return Err(crate::models::error::AppError::Message("Generated Bazi summary exceeded 200 characters".to_string()));
  }

  const REQUIRED_SEGMENTS: [&str; 5] = ["基础：", "｜做功：", "｜大运：", "｜性格：", "｜应期："];
  if REQUIRED_SEGMENTS.iter().any(|segment| !summary.contains(segment)) {
    return Err(crate::models::error::AppError::Message("Generated Bazi summary did not match the required format".to_string()));
  }

  const FORBIDDEN_TERMS: [&str; 9] = ["身强", "身弱", "旺衰", "扶抑", "调候", "从格", "用神", "忌神", "喜忌"];
  if FORBIDDEN_TERMS.iter().any(|term| summary.contains(term)) {
    return Err(crate::models::error::AppError::Message("Generated Bazi summary contained a forbidden balance-theory term".to_string()));
  }

  Ok(summary.to_string())
}

#[cfg(test)]
mod tests {
  use super::{resolve_birth_datetime, validate_bazi_summary};
  use chrono::{NaiveDate, Timelike};

  #[test]
  fn unknown_birth_time_uses_noon_reference_and_is_marked_unknown() {
    let date = NaiveDate::from_ymd_opt(1990, 1, 1).expect("test date must be valid");
    let (date_time, birth_time_known) = resolve_birth_datetime(date, None).expect("reference time must be valid");

    assert_eq!(date_time.date(), date);
    assert_eq!(date_time.hour(), 12);
    assert_eq!(date_time.minute(), 0);
    assert!(!birth_time_known);
  }

  #[test]
  fn validates_compact_bazi_summary_contract() {
    let valid = "基础：丁丑日;日时为主｜做功：子丑合，反局未载｜大运：未载｜性格：敏锐，易急｜应期：未载";
    assert_eq!(validate_bazi_summary(valid).expect("valid summary should pass"), valid);
    assert!(validate_bazi_summary("基础：身强｜做功：未载｜大运：未载｜性格：未载｜应期：未载").is_err());
  }
}
