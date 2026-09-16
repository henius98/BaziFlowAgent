use baziflow_agent::config::AppConfig;
use baziflow_agent::models::AppState;
use sqlx::SqlitePool;
use std::sync::Arc;

pub fn test_config(mock_url: String) -> AppConfig {
  AppConfig {
    runtime: Default::default(),
    upstreams: baziflow_agent::config::Upstreams { chart_base: mock_url.clone(), supplement_base: mock_url.clone(), almanac: format!("{mock_url}/api/almanac") },
    telegram_bot_token: "".into(),
    llm_client_config: baziflow_agent::services::llm::LlmClientConfig { api_key: "test".into(), api_base: mock_url, timeout_seconds: 30, http_client: None },
    llm_model_name: "gpt-4o".into(),
    database_url: "sqlite::memory:".into(),
    chat_cache_database_url: "sqlite::memory:".into(),
    chat_cache_max_messages: 40,
    user_contexts_expiration_minutes: 60,
    context_cleanup_cron: "".into(),
    log_cleanup_cron: "".into(),
    app_timezone: chrono_tz::Tz::UTC,
    log_retention_days: 7,
    max_context_messages: 10,
    base_url: "http://localhost".into(),
    log_level: "info".into(),
    cors_allowed_origin: "*".into(),
    r2_account_id: None,
    r2_access_key_id: None,
    r2_secret_access_key: None,
    r2_bucket_name: None,
  }
}

pub async fn test_state(config: Arc<AppConfig>, pool: SqlitePool) -> Arc<AppState> {
  let chat_cache = baziflow_agent::repos::chat_history_cache::init_chat_history_cache(&config.chat_cache_database_url, config.chat_cache_max_messages).await.unwrap();
  Arc::new(AppState::new(reqwest::Client::new(), pool, chat_cache.pool, chat_cache.writer, config).unwrap())
}
