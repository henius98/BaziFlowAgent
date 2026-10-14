use std::{
  sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
  },
  time::{Duration, Instant},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmissionRejection {
  ServerShuttingDown,
  MaxRequestsExceeded,
  RateLimitExceeded,
  UserAlreadyProcessing,
}

impl AdmissionRejection {
  pub fn user_message(&self) -> &'static str {
    match self {
      Self::ServerShuttingDown => "🛠️ 系统正在维护升级中，请稍后再试。",
      Self::MaxRequestsExceeded => "🚦 当前算力繁忙，系统已进入保护模式，请稍后重试。",
      Self::RateLimitExceeded => "⚠️ 请求过于频繁，请稍候（冷却中）。",
      Self::UserAlreadyProcessing => "⏳ 正在为您推算八字中，请等待上一条解读完成。",
    }
  }
}

#[derive(Debug)]
pub struct AdmissionPermit {
  pub user_id: u64,
  pub _request_permit: OwnedSemaphorePermit,
  pub _processing_guard: Option<UserProcessingGuard>,
}

#[derive(Debug)]
pub struct UserProcessingGuard {
  pub user_id: u64,
  pub active_users: Arc<dashmap::DashMap<u64, ()>>,
}

impl Drop for UserProcessingGuard {
  fn drop(&mut self) {
    self.active_users.remove(&self.user_id);
  }
}

#[derive(Debug)]
pub struct AdmissionController {
  pub requests: Arc<Semaphore>,
  pub active_users: Arc<dashmap::DashMap<u64, ()>>,
  pub rates: Arc<dashmap::DashMap<u64, (Instant, usize)>>,
  pub shutdown: CancellationToken,
}

impl AdmissionController {
  pub const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(3600 * 5); // 5 hours
  pub const RATE_LIMIT_RETRY_AFTER: &'static str = "18000";

  pub fn new(requests: Arc<Semaphore>, shutdown: CancellationToken) -> Self {
    Self { requests, active_users: Arc::new(dashmap::DashMap::new()), rates: Arc::new(dashmap::DashMap::new()), shutdown }
  }

  pub fn allow_at(&self, user_id: u64, limit: usize, now: Instant) -> bool {
    if self.rates.len() >= 4096 {
      self.rates.retain(|_, (start, _)| now.duration_since(*start) < Self::RATE_LIMIT_WINDOW);
      if self.rates.len() >= 4096 && !self.rates.contains_key(&user_id) {
        return false;
      }
    }
    let mut rate_entry = self.rates.entry(user_id).or_insert((now, 0));
    let (window_start, count) = rate_entry.value_mut();
    if now.duration_since(*window_start) >= Self::RATE_LIMIT_WINDOW {
      *window_start = now;
      *count = 0;
    }
    if *count >= limit {
      return false;
    }
    *count += 1;
    true
  }

  pub fn try_admit(&self, user_id: u64, rate_limit: usize, require_exclusive: bool) -> Result<AdmissionPermit, AdmissionRejection> {
    if self.shutdown.is_cancelled() {
      return Err(AdmissionRejection::ServerShuttingDown);
    }

    // 1. Concurrency Semaphore Check
    let request_permit = self.requests.clone().try_acquire_owned().map_err(|_| AdmissionRejection::MaxRequestsExceeded)?;

    // 2. Per-User Rate Limit Check
    let now = Instant::now();
    if !self.allow_at(user_id, rate_limit, now) {
      return Err(AdmissionRejection::RateLimitExceeded);
    }

    // 3. User Mutex / ProcessingGuard Check
    let processing_guard = if require_exclusive {
      if self.active_users.insert(user_id, ()).is_some() {
        return Err(AdmissionRejection::UserAlreadyProcessing);
      }
      Some(UserProcessingGuard { user_id, active_users: self.active_users.clone() })
    } else {
      None
    };

    Ok(AdmissionPermit { user_id, _request_permit: request_permit, _processing_guard: processing_guard })
  }
}

#[derive(Debug)]
pub struct Runtime {
  pub connections: Arc<Semaphore>,
  pub requests: Arc<Semaphore>,
  pub work: Arc<Semaphore>,
  pub llm: Arc<Semaphore>,
  pub shutdown: CancellationToken,
  pub tasks: TaskTracker,
  pub admission: Arc<AdmissionController>,
  draft_epoch: Instant,
  draft_next_ms: AtomicU64,
  user_limits: dashmap::DashMap<u64, (Instant, usize)>,
}

impl Runtime {
  pub fn new(config: &crate::config::RuntimeConfig) -> Self {
    let requests = Arc::new(Semaphore::new(config.max_requests));
    let shutdown = CancellationToken::new();
    let admission = Arc::new(AdmissionController::new(requests.clone(), shutdown.clone()));
    Self {
      connections: Arc::new(Semaphore::new(config.max_connections)),
      requests,
      work: Arc::new(Semaphore::new(config.max_work)),
      llm: Arc::new(Semaphore::new(config.max_work)),
      shutdown,
      tasks: TaskTracker::new(),
      admission,
      draft_epoch: Instant::now(),
      draft_next_ms: AtomicU64::new(0),
      user_limits: dashmap::DashMap::new(),
    }
  }

  /// Bounded identity limiter survives reconnects; fail closed if its table is full.
  pub fn allow(&self, user_id: u64, limit: usize) -> bool {
    self.allow_at(user_id, limit, Instant::now())
  }

  pub fn allow_at(&self, user_id: u64, limit: usize, now: Instant) -> bool {
    self.admission.allow_at(user_id, limit, now)
  }

  pub fn get_cached_limit(&self, user_id: u64) -> Option<usize> {
    let now = Instant::now();
    let entry = self.user_limits.get(&user_id)?;
    let (cached_at, limit) = *entry;
    if now.duration_since(cached_at) < Duration::from_secs(60) { Some(limit) } else { None }
  }

  pub fn set_cached_limit(&self, user_id: u64, limit: usize) {
    let now = Instant::now();
    if self.user_limits.len() >= 4096 {
      self.user_limits.retain(|_, (cached_at, _)| now.duration_since(*cached_at) < Duration::from_secs(60));
      if self.user_limits.len() >= 4096 && !self.user_limits.contains_key(&user_id) {
        return;
      }
    }
    self.user_limits.insert(user_id, (now, limit));
  }

  pub fn invalidate_cached_limit(&self, user_id: u64) {
    self.user_limits.remove(&user_id);
  }

  pub fn allow_draft(&self) -> bool {
    let now_ms = self.draft_epoch.elapsed().as_millis() as u64;
    let mut current = self.draft_next_ms.load(Ordering::Relaxed);
    loop {
      if now_ms < current {
        return false;
      }
      let next = now_ms.saturating_add(100);
      match self.draft_next_ms.compare_exchange_weak(current, next, Ordering::AcqRel, Ordering::Relaxed) {
        Ok(_) => return true,
        Err(actual) => current = actual,
      }
    }
  }

  pub fn pause_drafts(&self, seconds: u64) {
    let now_ms = self.draft_epoch.elapsed().as_millis() as u64;
    let pause_ms = seconds.min(3600).saturating_mul(1000);
    let target_ms = now_ms.saturating_add(pause_ms);
    let mut current = self.draft_next_ms.load(Ordering::Relaxed);
    loop {
      if current >= target_ms {
        break;
      }
      match self.draft_next_ms.compare_exchange_weak(current, target_ms, Ordering::AcqRel, Ordering::Relaxed) {
        Ok(_) => break,
        Err(actual) => current = actual,
      }
    }
  }

  pub fn try_work(&self) -> Option<OwnedSemaphorePermit> {
    if self.shutdown.is_cancelled() {
      return None;
    }
    self.work.clone().try_acquire_owned().ok()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  #[test]
  fn limiter_is_shared_and_expires() {
    let runtime = Runtime::new(&Default::default());
    let now = Instant::now();
    assert!(runtime.allow_at(1, 2, now));
    assert!(runtime.allow_at(1, 2, now));
    assert!(!runtime.allow_at(1, 2, now));
    assert!(runtime.allow_at(2, 2, now));
    assert!(runtime.allow_at(1, 2, now + AdmissionController::RATE_LIMIT_WINDOW));
  }

  #[test]
  fn user_limit_cache_respects_ttl_and_invalidation() {
    let runtime = Runtime::new(&Default::default());
    assert_eq!(runtime.get_cached_limit(1), None);

    runtime.set_cached_limit(1, 42);
    assert_eq!(runtime.get_cached_limit(1), Some(42));

    runtime.invalidate_cached_limit(1);
    assert_eq!(runtime.get_cached_limit(1), None);
  }
}
