use crate::config::AppConfig;
use chrono::Utc;
use dashmap::DashMap;
use std::sync::{Arc, OnceLock};

#[derive(Debug, Clone, Default)]
pub struct PickState {
  pub start_date: Option<String>,
  pub end_date: Option<String>,
  pub activity: Option<String>,
  pub waiting_for_text: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ProfileState {
  pub gender: Option<u8>,
  pub birthdate: Option<String>,
  pub hour: Option<u8>,
  pub minute: Option<u8>,
  pub location: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UserContext {
  pub messages: Vec<String>,
  /// Whether `messages` has been restored from the persistent cache for this session.
  pub history_loaded: bool,
  pub last_active: chrono::DateTime<Utc>,
  pub last_request_at: chrono::DateTime<Utc>,
  pub is_processing: bool,

  pub profile_state: ProfileState,
  pub pick_state: PickState,
}

impl Default for UserContext {
  fn default() -> Self {
    Self {
      messages: Vec::new(),
      history_loaded: false,
      last_active: chrono::Utc::now(),
      last_request_at: chrono::DateTime::<Utc>::MIN_UTC,
      is_processing: false,

      profile_state: ProfileState::default(),
      pick_state: PickState::default(),
    }
  }
}

impl UserContext {
  /// Append a message, evicting the oldest turn when at capacity.
  pub fn push_message(&mut self, msg: String, max: usize) {
    if max == 0 || msg.len() > 131_072 {
      return;
    }
    while !self.messages.is_empty() && (self.messages.len() >= max || self.messages.iter().map(String::len).sum::<usize>() + msg.len() > 131_072) {
      self.messages.remove(0);
    }
    self.messages.push(msg);
  }
}

pub static GLOBAL_STATE: OnceLock<Arc<AppState>> = OnceLock::new();
pub fn get_state() -> Arc<AppState> {
  GLOBAL_STATE.get().cloned().unwrap_or_else(|| {
    tracing::error!("CRITICAL: AppState not initialized");
    std::process::exit(1);
  })
}

#[cfg(test)]
pub fn set_state_for_test(state: Arc<AppState>) {
  // We ignore the Result of set since it might already be set in another test.
  let _ = GLOBAL_STATE.set(state);
}

/// Shared application state structure
#[derive(Debug)]
pub struct AppState {
  pub runtime: Arc<crate::models::runtime::Runtime>,
  pub events: Arc<crate::models::events::EventHub>,
  pub http_client: reqwest::Client,
  pub db_pool: sqlx::SqlitePool,
  /// Dedicated, disposable SQLite store for recent chat turns.
  pub chat_cache_pool: sqlx::SqlitePool,
  /// Non-blocking, batched writer for the chat cache.
  pub chat_cache_writer: crate::repos::chat_history_cache::ChatHistoryWriter,
  pub config: Arc<AppConfig>,

  /// Global dictionary to store user contexts and pending inputs
  pub user_contexts: DashMap<u64, UserContext>,

  /// Track user-specific background jobs (user_id -> Job UUID)
  pub user_jobs: DashMap<u64, uuid::Uuid>,

  /// Cloudflare R2 bucket client (if configured)
  pub r2_bucket: Option<s3::Bucket>,

  /// LLM Service for decoupled AI generation
  pub llm_service: Arc<dyn crate::services::llm::LlmService>,
}

impl AppState {
  pub fn new(
    http_client: reqwest::Client,
    db_pool: sqlx::SqlitePool,
    chat_cache_pool: sqlx::SqlitePool,
    chat_cache_writer: crate::repos::chat_history_cache::ChatHistoryWriter,
    config: Arc<AppConfig>,
  ) -> crate::models::error::AppResult<Self> {
    let r2_bucket =
      if let (Some(account_id), Some(access_key), Some(secret_key), Some(bucket_name)) = (&config.r2_account_id, &config.r2_access_key_id, &config.r2_secret_access_key, &config.r2_bucket_name) {
        let creds = s3::creds::Credentials::new(Some(access_key), Some(secret_key), None, None, None)?;
        let region = s3::region::Region::Custom { region: "auto".to_owned(), endpoint: format!("https://{}.r2.cloudflarestorage.com", account_id) };
        Some(s3::Bucket::new(bucket_name, region, creds)?.with_path_style())
      } else {
        None
      };

    let runtime = Arc::new(crate::models::runtime::Runtime::new(&config.runtime));
    let llm_service = Arc::new(crate::services::llm::DefaultLlmService { runtime: runtime.clone(), pool: db_pool.clone(), config: config.llm_client_config.clone() });

    Ok(Self {
      runtime,
      events: Arc::new(crate::models::events::EventHub::default()),
      http_client,
      db_pool,
      chat_cache_pool,
      chat_cache_writer,
      config,
      user_contexts: DashMap::new(),
      user_jobs: DashMap::new(),
      r2_bucket,
      llm_service,
    })
  }
}
