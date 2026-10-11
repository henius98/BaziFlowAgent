use crate::models::AppState;
use crate::models::runtime::{AdmissionPermit, AdmissionRejection};
use std::sync::Arc;

pub struct ProcessingGuard {
  user_id: u64,
  state: Arc<AppState>,
  _permit: tokio::sync::OwnedSemaphorePermit,
  _admission_permit: Option<AdmissionPermit>,
}

impl ProcessingGuard {
  pub fn try_acquire(state: Arc<AppState>, user_id: u64) -> Result<Self, AdmissionRejection> {
    if state.runtime.shutdown.is_cancelled() {
      return Err(AdmissionRejection::ServerShuttingDown);
    }
    let permit = state.runtime.try_work().ok_or(AdmissionRejection::MaxRequestsExceeded)?;
    if state.user_contexts.len() >= state.config.runtime.max_contexts && !state.user_contexts.contains_key(&user_id) {
      return Err(AdmissionRejection::MaxRequestsExceeded);
    }
    let admission_permit = state.runtime.admission.try_admit(user_id, state.config.runtime.requests_per_minute, true)?;
    {
      let mut ctx = state.user_contexts.entry(user_id).or_default();
      if ctx.is_processing {
        return Err(AdmissionRejection::UserAlreadyProcessing);
      }
      ctx.is_processing = true;
      ctx.last_active = chrono::Utc::now();
    }
    Ok(Self { user_id, state, _permit: permit, _admission_permit: Some(admission_permit) })
  }

  pub fn acquire(state: Arc<AppState>, user_id: u64) -> Option<Self> {
    Self::try_acquire(state, user_id).ok()
  }
}

impl Drop for ProcessingGuard {
  fn drop(&mut self) {
    if let Some(mut ctx) = self.state.user_contexts.get_mut(&self.user_id) {
      ctx.is_processing = false;
    }
  }
}
