use std::{env, net::SocketAddr};

/// Process-wide resource budgets. Shared by every transport.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
  pub bind: SocketAddr,
  pub max_connections: usize,
  pub max_requests: usize,
  pub max_work: usize,
  pub max_contexts: usize,
  pub max_message_bytes: usize,
  pub queue_capacity: usize,
  pub requests_per_minute: usize,
  pub idle_seconds: u64,
  pub send_seconds: u64,
  pub work_seconds: u64,
  pub shutdown_seconds: u64,
}

impl Default for RuntimeConfig {
  fn default() -> Self {
    Self {
      bind: SocketAddr::from(([127, 0, 0, 1], 8080)),
      max_connections: 256,
      max_requests: 64,
      max_work: 16,
      max_contexts: 256,
      max_message_bytes: 16_384,
      queue_capacity: 32,
      requests_per_minute: 120,
      idle_seconds: 60,
      send_seconds: 5,
      work_seconds: 180,
      shutdown_seconds: 30,
    }
  }
}

impl RuntimeConfig {
  pub fn from_env() -> anyhow::Result<Self> {
    let mut value = Self::default();
    if let Ok(bind) = env::var("HTTP_BIND_ADDRESS") {
      value.bind = bind.parse().map_err(|_| anyhow::anyhow!("Invalid HTTP_BIND_ADDRESS"))?;
    }
    macro_rules! setting {
      ($field:ident, $name:literal, $max:expr) => {
        if let Ok(raw) = env::var($name) {
          value.$field = raw.parse().map_err(|_| anyhow::anyhow!(concat!("Invalid ", $name)))?;
        }
        anyhow::ensure!(value.$field > 0 && value.$field <= $max, concat!($name, " is outside its supported range"));
      };
    }
    setting!(max_connections, "WS_MAX_CONNECTIONS", 10_000);
    setting!(max_requests, "API_MAX_REQUESTS", 1024);
    setting!(max_contexts, "APP_MAX_CONTEXTS", 4096);
    setting!(max_work, "APP_MAX_WORK", 256);
    setting!(max_message_bytes, "API_MAX_MESSAGE_BYTES", 65_536);
    setting!(queue_capacity, "WS_QUEUE_CAPACITY", 256);
    setting!(requests_per_minute, "API_REQUESTS_PER_MINUTE", 6000);
    setting!(idle_seconds, "WS_IDLE_SECONDS", 300);
    setting!(send_seconds, "WS_SEND_SECONDS", 30);
    setting!(work_seconds, "APP_WORK_SECONDS", 600);
    setting!(shutdown_seconds, "SHUTDOWN_SECONDS", 120);
    Ok(value)
  }
}
