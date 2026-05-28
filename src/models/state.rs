use crate::config::AppConfig;
use chrono::Utc;
use dashmap::DashMap;
use std::sync::{Arc, OnceLock};

#[derive(Debug, Clone)]
pub struct UserContext {
    pub messages: Vec<String>,
    pub gender: Option<u8>,
    pub birthdate: Option<String>,
    pub hour: Option<u8>,
    pub minute: Option<u8>,
    pub location: Option<String>,
    pub last_active: chrono::DateTime<Utc>,
}

impl Default for UserContext {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            gender: None,
            birthdate: None,
            location: None,
            hour: None,
            minute: None,
            last_active: chrono::Utc::now(),
        }
    }
}

pub static GLOBAL_STATE: OnceLock<Arc<AppState>> = OnceLock::new();
pub fn get_state() -> Arc<AppState> {
    GLOBAL_STATE.get().expect("AppState not initialized").clone()
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
