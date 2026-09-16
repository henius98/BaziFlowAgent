use std::{
  collections::HashMap,
  sync::{Arc, Mutex},
  time::{Duration, Instant},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

#[derive(Debug)]
pub struct Runtime {
  pub connections: Arc<Semaphore>,
  pub requests: Arc<Semaphore>,
  pub work: Arc<Semaphore>,
  pub llm: Arc<Semaphore>,
  pub shutdown: CancellationToken,
  pub tasks: TaskTracker,
  draft_next: Mutex<Instant>,
  rates: Mutex<HashMap<u64, (Instant, usize)>>,
}

impl Runtime {
  pub fn new(config: &crate::config::RuntimeConfig) -> Self {
    Self {
      connections: Arc::new(Semaphore::new(config.max_connections)),
      requests: Arc::new(Semaphore::new(config.max_requests)),
      work: Arc::new(Semaphore::new(config.max_work)),
      llm: Arc::new(Semaphore::new(config.max_work)),
      shutdown: CancellationToken::new(),
      tasks: TaskTracker::new(),
      draft_next: Mutex::new(Instant::now()),
      rates: Mutex::new(HashMap::new()),
    }
  }

  /// Bounded identity limiter survives reconnects; fail closed if its table is full.
  pub fn allow(&self, user_id: u64, limit: usize) -> bool {
    self.allow_at(user_id, limit, Instant::now())
  }

  fn allow_at(&self, user_id: u64, limit: usize, now: Instant) -> bool {
    let Ok(mut rates) = self.rates.lock() else {
      return false;
    };
    if rates.len() >= 4096 {
      rates.retain(|_, (start, _)| now.duration_since(*start) < Duration::from_secs(60));
      if rates.len() >= 4096 && !rates.contains_key(&user_id) {
        return false;
      }
    }
    let (start, count) = rates.entry(user_id).or_insert((now, 0));
    if now.duration_since(*start) >= Duration::from_secs(60) {
      *start = now;
      *count = 0;
    }
    if *count >= limit {
      return false;
    }
    *count += 1;
    true
  }

  pub fn allow_draft(&self) -> bool {
    let Ok(mut next) = self.draft_next.lock() else {
      return false;
    };
    let now = Instant::now();
    if now < *next {
      return false;
    }
    *next = now + Duration::from_millis(100);
    true
  }

  pub fn pause_drafts(&self, seconds: u64) {
    if let Ok(mut next) = self.draft_next.lock() {
      *next = Instant::now() + Duration::from_secs(seconds.min(3600));
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
    assert!(runtime.allow_at(1, 2, now + Duration::from_secs(60)));
  }
}
