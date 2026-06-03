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

    /// Track user-specific background jobs (user_id -> Job UUID)
    pub user_jobs: DashMap<u64, uuid::Uuid>,

    /// Cloudflare R2 bucket client (if configured)
    pub r2_bucket: Option<s3::Bucket>,
}

impl AppState {
    pub fn new(http_client: reqwest::Client, db_pool: sqlx::SqlitePool, config: Arc<AppConfig>) -> Self {
        let r2_bucket = if let (Some(account_id), Some(access_key), Some(secret_key), Some(bucket_name)) =
            (&config.r2_account_id, &config.r2_access_key_id, &config.r2_secret_access_key, &config.r2_bucket_name)
        {
            let creds = s3::creds::Credentials::new(Some(access_key), Some(secret_key), None, None, None).expect("Failed to create R2 credentials");
            let region = s3::region::Region::Custom {
                region: "auto".to_owned(),
                endpoint: format!("https://{}.r2.cloudflarestorage.com", account_id),
            };
            Some(s3::Bucket::new(bucket_name, region, creds).expect("Failed to initialize R2 bucket client").with_path_style())
        } else {
            None
        };

        Self {
            http_client,
            db_pool,
            config,
            user_contexts: DashMap::new(),
            user_jobs: DashMap::new(),
            r2_bucket,
        }
    }
}
