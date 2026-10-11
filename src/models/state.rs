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

use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatRole {
  User,
  Assistant,
  Tool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatTurn {
  pub role: ChatRole,
  pub content: Arc<str>,
}

#[derive(Debug, Clone)]
pub struct UserContext {
  pub messages: VecDeque<ChatTurn>,
  pub total_bytes: usize,
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
      messages: VecDeque::new(),
      total_bytes: 0,
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
  pub const MAX_TOTAL_BYTES: usize = 131_072;

  /// Append a structured turn, evicting the oldest turn when at capacity.
  pub fn push_turn(&mut self, role: ChatRole, content: impl Into<Arc<str>>, max_turns: usize) {
    let content: Arc<str> = content.into();
    let turn_len = content.len();

    if max_turns == 0 || turn_len > Self::MAX_TOTAL_BYTES {
      return;
    }

    while !self.messages.is_empty() && (self.messages.len() >= max_turns || self.total_bytes + turn_len > Self::MAX_TOTAL_BYTES) {
      if let Some(evicted) = self.messages.pop_front() {
        self.total_bytes = self.total_bytes.saturating_sub(evicted.content.len());
      }
    }

    self.total_bytes += turn_len;
    self.messages.push_back(ChatTurn { role, content });
  }

  /// Append a message, evicting the oldest turn when at capacity.
  pub fn push_message(&mut self, msg: String, max: usize) {
    let (role, content) = if let Some(stripped) = msg.strip_prefix("User: ") {
      (ChatRole::User, stripped)
    } else if let Some(stripped) = msg.strip_prefix("Assistant: ") {
      (ChatRole::Assistant, stripped)
    } else if let Some(stripped) = msg.strip_prefix("Tool: ") {
      (ChatRole::Tool, stripped)
    } else {
      (ChatRole::User, msg.as_str())
    };
    self.push_turn(role, content, max);
  }

  /// Clear all messages and reset byte tracking.
  pub fn clear_messages(&mut self) {
    self.messages.clear();
    self.total_bytes = 0;
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
  pub user_repo: Arc<dyn crate::repos::UserRepository>,
  pub log_repo: Arc<dyn crate::repos::LlmLogRepository>,
  pub chat_cache: Arc<dyn crate::repos::ChatCacheRepository>,
  pub blob_store: Arc<dyn crate::repos::BlobStore>,
  pub db_pool: crate::repos::Database,
  /// Dedicated, disposable SQLite store for recent chat turns.
  pub chat_cache_pool: sqlx::SqlitePool,
  /// Non-blocking, batched writer for the chat cache.
  pub chat_cache_writer: crate::repos::chat_history_cache::ChatHistoryWriter,
  pub config: Arc<AppConfig>,

  /// Global dictionary to store user contexts and pending inputs
  pub user_contexts: DashMap<u64, UserContext>,

  /// Track user-specific background jobs (user_id -> Job UUID)
  pub user_jobs: DashMap<u64, uuid::Uuid>,

  /// LLM Service for decoupled AI generation
  pub llm_service: Arc<dyn crate::services::llm::LlmService>,
}

impl AppState {
  pub fn new(
    http_client: reqwest::Client,
    db_pool: impl Into<crate::repos::Database>,
    chat_cache_pool: sqlx::SqlitePool,
    chat_cache_writer: crate::repos::chat_history_cache::ChatHistoryWriter,
    config: Arc<AppConfig>,
  ) -> crate::models::error::AppResult<Self> {
    let db_pool = db_pool.into();
    let user_repo: Arc<dyn crate::repos::UserRepository> = Arc::new(db_pool.clone());
    let log_repo: Arc<dyn crate::repos::LlmLogRepository> = Arc::new(db_pool.clone());
    let chat_cache: Arc<dyn crate::repos::ChatCacheRepository> = Arc::new(crate::repos::chat_history_cache::ChatHistoryCache { pool: chat_cache_pool.clone(), writer: chat_cache_writer.clone() });
    let blob_store = crate::repos::init_blob_store(&config)?;

    let runtime = Arc::new(crate::models::runtime::Runtime::new(&config.runtime));
    let llm_service = Arc::new(crate::services::llm::DefaultLlmService { runtime: runtime.clone(), pool: db_pool.clone(), config: config.llm_client_config.clone() });

    Ok(Self {
      runtime,
      events: Arc::new(crate::models::events::EventHub::default()),
      http_client,
      user_repo,
      log_repo,
      chat_cache,
      blob_store,
      db_pool,
      chat_cache_pool,
      chat_cache_writer,
      config,
      user_contexts: DashMap::new(),
      user_jobs: DashMap::new(),
      llm_service,
    })
  }

  pub async fn get_user_rate_limit(&self, user_id: u64) -> usize {
    if user_id == 0 {
      return self.config.runtime.requests_per_minute;
    }
    if let Some(limit) = self.runtime.get_cached_limit(user_id) {
      return limit;
    }
    let limit = crate::repos::get_user_rate_limit(&self.db_pool, user_id).await.unwrap_or(self.config.runtime.requests_per_minute);
    self.runtime.set_cached_limit(user_id, limit);
    limit
  }
}
