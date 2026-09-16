use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime};
use tracing::error;

use crate::repos;
use crate::services::{paipan, solar_time};

pub struct BaziDataParams<'a> {
  pub user_id: u64,
  pub username: &'a str,
  pub birth_date: &'a str,
  pub birth_time: Option<NaiveTime>,
  pub gender: u8,
  pub location: Option<String>,
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

pub async fn prepare_bazi_data(state: &std::sync::Arc<crate::models::AppState>, params: BaziDataParams<'_>) -> crate::models::AppResult<paipan::StructuredBazi> {
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

  // Calculate True Solar Time only when the location is known.
  let solar_dt = if let Some(city_name) = &params.location { solar_time::calculate_true_solar_time(naive_dt, city_name, 120.0) } else { naive_dt };

  let birth_year = naive_dt.year();
  let (structured_data, structured_json) = paipan::fetch_bazi_chart(&state.http_client, &state.config.upstreams, solar_dt, params.gender, birth_year, params.location, birth_time_known).await?;

  repos::upsert_user_bazi(&state.db_pool, params.user_id, Some(params.username), &structured_json, params.gender, params.birth_date).await?;

  Ok(structured_data)
}

pub async fn build_and_save_bazi_html(state: &std::sync::Arc<crate::models::AppState>, user_id: u64, username: &str, structured_data: &paipan::StructuredBazi) {
  use rand::Rng;
  let token = hex::encode(rand::rng().random::<[u8; 32]>());
  if sqlx::query("UPDATE users SET chart_token = ?2 WHERE user_id = ?1").bind(user_id as i64).bind(&token).execute(&state.db_pool).await.is_err() {
    error!("Failed to rotate chart capability");
    return;
  }
  state.events.publish(user_id, "profile.updated", "application");
  let html_diagram = paipan::generate_bazi_html(structured_data, username);
  let filename = format!("bazi_{}.html", user_id);

  if let Some(bucket) = &state.r2_bucket {
    if let Err(e) = bucket.put_object(&filename, html_diagram.as_bytes()).await {
      error!("Failed to upload Bazi HTML to R2: {}", e);
    }
  } else {
    let public_path = std::path::PathBuf::from("public").join(&filename);
    if let Err(e) = tokio::fs::write(&public_path, html_diagram).await {
      error!("Failed to save Bazi HTML to public: {}", e);
    }
  }
}

pub async fn get_bazi_chart_url(state: &std::sync::Arc<crate::models::AppState>, user_id: u64) -> crate::models::AppResult<String> {
  let filename = format!("bazi_{}.html", user_id);
  if let Some(bucket) = &state.r2_bucket {
    // Presign URL valid for 24 hours (86400 seconds)
    bucket.presign_get(&filename, 86400, None).await.map_err(|e| crate::models::error::AppError::Message(format!("Failed to generate presigned URL: {}", e)))
  } else {
    let token: Option<String> = sqlx::query_scalar("SELECT chart_token FROM users WHERE user_id = ?1").bind(user_id as i64).fetch_optional(&state.db_pool).await?.flatten();
    let token = token.ok_or_else(|| crate::models::AppError::Message("Chart unavailable; regenerate the profile".into()))?;
    Ok(format!("{}/charts/{}", state.config.base_url.trim_end_matches('/'), token))
  }
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

  let model_name = llm_model.map(|m| m.as_str().to_string()).unwrap_or_else(|| state.config.llm_model_name.clone());
  let mut params = crate::services::llm::LlmRequestParams::new(model_name, vec![system_message.into(), user_message.into()]);
  params.temperature = Some(0.2);
  params.top_p = Some(0.75);
  params.stream = Some(true);
  params.user_id = Some(user_id as i64);

  match state.llm_service.call(params).await {
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

  let model_name = llm_model.map(|m| m.as_str().to_string()).unwrap_or_else(|| state.config.llm_model_name.clone());
  let mut params = crate::services::llm::LlmRequestParams::new(model_name, vec![system_message.into(), user_message.into()]);
  params.temperature = Some(0.1);
  params.top_p = Some(0.7);
  params.stream = Some(false);
  params.user_id = Some(user_id as i64);
  params.request_type = Some(crate::models::LlmRequestType::GenerateBaziSummary);

  match state.llm_service.call(params).await {
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
    let valid = "基础：丁丑日；日时为主｜做功：子丑合，反局未载｜大运：未载｜性格：敏锐，易急｜应期：未载";
    assert_eq!(validate_bazi_summary(valid).expect("valid summary should pass"), valid);
    assert!(validate_bazi_summary("基础：身强｜做功：未载｜大运：未载｜性格：未载｜应期：未载").is_err());
  }
}
