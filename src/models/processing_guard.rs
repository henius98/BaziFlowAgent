use crate::models::AppState;
use std::sync::Arc;

pub struct ProcessingGuard {
  user_id: u64,
  state: Arc<AppState>,
  _permit: tokio::sync::OwnedSemaphorePermit,
}

impl ProcessingGuard {
  pub fn acquire(state: Arc<AppState>, user_id: u64) -> Option<Self> {
    let permit = state.runtime.try_work()?;
    if state.user_contexts.len() >= state.config.runtime.max_contexts && !state.user_contexts.contains_key(&user_id) {
      return None;
    }
    {
      let mut ctx = state.user_contexts.entry(user_id).or_default();
      if ctx.is_processing {
        return None;
      }
      ctx.is_processing = true;
      ctx.last_active = chrono::Utc::now();
    }
    Some(Self { user_id, state, _permit: permit })
  }
}

impl Drop for ProcessingGuard {
  fn drop(&mut self) {
    if let Some(mut ctx) = self.state.user_contexts.get_mut(&self.user_id) {
      ctx.is_processing = false;
    }
  }
}
