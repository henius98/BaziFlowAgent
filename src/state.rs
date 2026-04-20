use chrono::Utc;
use dashmap::DashMap;
use std::sync::Arc;

/// Shared application state structure.
/// Separating this allows other modules to easily import without depending on `main`,
/// and serves as a good boilerplate for handling shared state in app architecture.
pub struct AppState {
    pub http_client: reqwest::Client,
    pub db_pool: sqlx::SqlitePool,
    pub openai_api_key: String,
    pub openai_api_base: String,
    pub llm_model_name: String,
    pub user_bazi: String,
    /// Global dictionary to store user messages
    pub user_contexts: Arc<DashMap<i64, Vec<String>>>,
    /// Track when users were last active
    pub user_last_active: Arc<DashMap<i64, chrono::DateTime<Utc>>>,
    /// Temporary storage for user's selected gender during /new flow
    pub pending_gender: Arc<DashMap<i64, u8>>,
    /// Temporary storage for user's selected date (YYYY-MM-DD) before time is selected via Telegram numpad
    pub pending_birthdate: Arc<DashMap<i64, String>>,
    pub max_context_messages: usize,
}

impl AppState {
    pub fn new(
        http_client: reqwest::Client,
        db_pool: sqlx::SqlitePool,
        openai_api_key: String,
        openai_api_base: String,
        llm_model_name: String,
        user_bazi: String,
        max_context_messages: usize,
    ) -> Self {
        Self {
            http_client,
            db_pool,
            openai_api_key,
            openai_api_base,
            llm_model_name,
            user_bazi,
            user_contexts: Arc::new(DashMap::new()),
            user_last_active: Arc::new(DashMap::new()),
            pending_gender: Arc::new(DashMap::new()),
            pending_birthdate: Arc::new(DashMap::new()),
            max_context_messages,
        }
    }
}
