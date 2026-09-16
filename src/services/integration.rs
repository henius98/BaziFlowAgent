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
  let data = crate::repos::get_user_snapshot(&state.db_pool, owner).await?;
  Ok(Snapshot { has_profile: data.has_profile, model: data.model, schedule: data.schedule })
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
