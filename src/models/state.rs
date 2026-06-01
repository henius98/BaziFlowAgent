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
    pub last_active: chrono::DateTime<Utc>,

    pub profile_state: ProfileState,
    pub pick_state: PickState,
}

impl Default for UserContext {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            last_active: chrono::Utc::now(),

            profile_state: ProfileState::default(),
            pick_state: PickState::default(),
        }
    }
}

impl UserContext {
    /// Append a message, evicting the oldest Q&A pair if at capacity.
    /// Keeps index 0 (system context) intact and removes pairs from index 1.
    pub fn push_message(&mut self, msg: String, max: usize) {
        if self.messages.len() >= max {
            self.messages.remove(1);
            self.messages.remove(1);
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

/// Shared application state structure
#[derive(Debug)]
pub struct AppState {
    pub http_client: reqwest::Client,
    pub db_pool: sqlx::SqlitePool,
    pub config: Arc<AppConfig>,

    /// Global dictionary to store user contexts and pending inputs
    pub user_contexts: DashMap<u64, UserContext>,
}

impl AppState {
    pub fn new(http_client: reqwest::Client, db_pool: sqlx::SqlitePool, config: Arc<AppConfig>) -> Self {
        Self {
            http_client,
            db_pool,
            config,
            user_contexts: DashMap::new(),
        }
    }
}
