//! Owner-scoped application commands shared with integration transports.
use crate::models::{AppResult, AppState};
use serde::Serialize;

#[derive(Serialize)]
pub struct Snapshot {
  pub has_profile: bool,
  pub model: Option<u8>,
  pub schedule: Option<String>,
}

pub async fn snapshot(state: &AppState, owner: u64) -> AppResult<Snapshot> {
  let row = sqlx::query_as::<_, (bool, Option<u8>, Option<String>)>("SELECT bazi_four_pillars IS NOT NULL, llm_model, schedule FROM users WHERE user_id = ?1")
    .bind(owner as i64)
    .fetch_optional(&state.db_pool)
    .await?;
  let (has_profile, model, schedule) = row.unwrap_or((false, None, None));
  Ok(Snapshot { has_profile, model, schedule })
}

/// Setting a value is naturally idempotent across retries and reconnects.
pub async fn set_model(state: &AppState, owner: u64, model: u8) -> AppResult<()> {
  if crate::models::common::LlmModel::from_u8(model).is_none() {
    return Err(crate::models::AppError::Message("Invalid model".into()));
  }
  crate::repos::update_user_llm_model(&state.db_pool, owner, model).await?;
  state.events.publish(owner, "profile.updated", "application");
  Ok(())
}
