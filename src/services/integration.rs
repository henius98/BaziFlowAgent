//! Owner-scoped application commands shared with integration transports.
use crate::models::{AppResult, AppState};

pub use crate::repos::UserSnapshotData as Snapshot;

pub async fn snapshot(state: &AppState, owner: u64) -> AppResult<Snapshot> {
  crate::repos::get_user_snapshot(&state.db_pool, owner).await
}

/// Setting a value is naturally idempotent across retries and reconnects.
pub async fn set_model(state: &AppState, owner: u64, model: u8) -> AppResult<crate::models::common::LlmModel> {
  let saved_model = crate::repos::update_user_llm_model(&state.db_pool, owner, model).await?;
  state.events.publish(owner, "profile.updated", "application");
  Ok(saved_model)
}
