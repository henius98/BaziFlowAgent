use crate::config::AppConfig;
use chrono::Utc;
use dashmap::DashMap;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct PickState {
  pub start_date: Option<String>,
  pub end_date: Option<String>,
  pub activity: Option<String>,
  pub waiting_for_text: bool,
  pub last_active: std::time::Instant,
}

impl Default for PickState {
  fn default() -> Self {
    Self { start_date: None, end_date: None, activity: None, waiting_for_text: false, last_active: std::time::Instant::now() }
  }
}

#[derive(Debug, Clone)]
pub struct ProfileState {
  pub gender: Option<u8>,
  pub birthdate: Option<String>,
  pub hour: Option<u8>,
  pub minute: Option<u8>,
  pub location: Option<String>,
  pub last_active: std::time::Instant,
}

impl Default for ProfileState {
  fn default() -> Self {
    Self { gender: None, birthdate: None, hour: None, minute: None, location: None, last_active: std::time::Instant::now() }
  }
}

use std::collections::VecDeque;

pub use crate::models::common::ChatRole;

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
}

impl Default for UserContext {
  fn default() -> Self {
    Self { messages: VecDeque::new(), total_bytes: 0, history_loaded: false, last_active: chrono::Utc::now(), last_request_at: chrono::DateTime::<Utc>::MIN_UTC, is_processing: false }
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

// Global state singletons eliminated in favor of explicit dependency injection.
// Previously set_state_for_test; state is now explicitly injected into tests and handlers.

/// Shared application state structure
pub struct AppState {
  pub runtime: Arc<crate::models::runtime::Runtime>,
  pub events: Arc<crate::models::events::EventHub>,
  pub http_client: reqwest::Client,
  pub chat_cache: Arc<crate::repos::chat_history_cache::ChatHistoryCache>,
  pub blob_store: Arc<dyn crate::repos::BlobStore>,
  pub db_pool: crate::repos::Database,
  /// Dedicated, disposable SQLite store for recent chat turns.
  pub chat_cache_pool: sqlx::SqlitePool,
  /// Non-blocking, batched writer for the chat cache.
  pub chat_cache_writer: crate::repos::chat_history_cache::ChatHistoryWriter,
  pub config: Arc<AppConfig>,

  /// Centrally throttled Telegram bot instance for scheduled and outbound delivery.
  pub bot: Option<crate::bot::Bot>,

  /// Background job scheduler for daily fortune delivery and cache cleanup.
  pub scheduler: Option<tokio_cron_scheduler::JobScheduler>,

  /// Global dictionary to store user contexts and pending inputs
  pub user_contexts: DashMap<u64, UserContext>,

  /// Telegram bot UI wizard state for /new profile creation flow
  pub profile_wizards: DashMap<u64, ProfileState>,

  /// Telegram bot UI wizard state for /date selection flow
  pub pick_wizards: DashMap<u64, PickState>,

  /// Track user-specific background jobs (user_id -> Job UUID)
  pub user_jobs: DashMap<u64, uuid::Uuid>,

  /// External adapter for traditional Chinese almanac retrieval.
  pub almanac_provider: Arc<dyn crate::services::almanac::AlmanacProvider>,

  /// External adapter for Paipan Bazi calculation and chart retrieval.
  pub chart_provider: Arc<dyn crate::services::paipan::ChartProvider>,

  /// External adapter for LLM chat completions and streaming.
  pub llm_service: Arc<dyn crate::services::llm::LlmService>,
}

impl std::fmt::Debug for AppState {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("AppState")
      .field("runtime", &self.runtime)
      .field("events", &self.events)
      .field("config", &self.config)
      .field("almanac_provider", &self.almanac_provider)
      .field("chart_provider", &self.chart_provider)
      .field("llm_service", &self.llm_service)
      .finish_non_exhaustive()
  }
}

/// LLM Service for decoupled AI generation is invoked via AppState.llm_service or crate::services::llm::call_llm.
impl AppState {
  pub fn new(
    http_client: reqwest::Client,
    db_pool: impl Into<crate::repos::Database>,
    chat_cache_pool: sqlx::SqlitePool,
    chat_cache_writer: crate::repos::chat_history_cache::ChatHistoryWriter,
    config: Arc<AppConfig>,
    bot: Option<crate::bot::Bot>,
    scheduler: Option<tokio_cron_scheduler::JobScheduler>,
  ) -> crate::models::error::AppResult<Self> {
    let db_pool = db_pool.into();
    let chat_cache = Arc::new(crate::repos::chat_history_cache::ChatHistoryCache { pool: chat_cache_pool.clone(), writer: chat_cache_writer.clone() });
    let blob_store = crate::repos::init_blob_store(&config)?;

    let almanac_provider = Arc::new(crate::services::almanac::HttpAlmanacProvider::new(http_client.clone(), config.upstreams.almanac.clone()));
    let chart_provider = Arc::new(crate::services::paipan::HttpChartProvider::new(http_client.clone(), config.upstreams.clone()));
    let llm_service = Arc::new(crate::services::llm::DefaultLlmService::new(config.llm_client_config.clone()));

    let runtime = Arc::new(crate::models::runtime::Runtime::new(&config.runtime));

    Ok(Self {
      runtime,
      events: Arc::new(crate::models::events::EventHub::default()),
      http_client,
      chat_cache,
      blob_store,
      db_pool,
      chat_cache_pool,
      chat_cache_writer,
      config,
      bot,
      scheduler,
      almanac_provider,
      chart_provider,
      llm_service,
      user_contexts: DashMap::new(),
      profile_wizards: DashMap::new(),
      pick_wizards: DashMap::new(),
      user_jobs: DashMap::new(),
    })
  }

  /// Override external adapters for testing or alternate providers.
  pub fn with_adapters(
    mut self,
    almanac_provider: Arc<dyn crate::services::almanac::AlmanacProvider>,
    chart_provider: Arc<dyn crate::services::paipan::ChartProvider>,
    llm_service: Arc<dyn crate::services::llm::LlmService>,
  ) -> Self {
    self.almanac_provider = almanac_provider;
    self.chart_provider = chart_provider;
    self.llm_service = llm_service;
    self
  }

  pub async fn get_user_rate_limit(&self, user_id: u64) -> usize {
    if user_id == 0 {
      return self.config.runtime.rate_limit;
    }
    if let Some(limit) = self.runtime.get_cached_limit(user_id) {
      return limit;
    }
    let limit = crate::repos::get_user_rate_limit(&self.db_pool, user_id).await.unwrap_or(self.config.runtime.rate_limit);
    self.runtime.set_cached_limit(user_id, limit);
    limit
  }
}
