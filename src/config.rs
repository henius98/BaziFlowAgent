use std::env;

/// Application configuration structure loaded from environment variables.
/// This can be used as a template for other projects to centralize configuration loading.
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub telegram_bot_token: String,
    pub openai_api_key: String,
    pub openai_api_base: String,
    pub llm_model_name: String,
    pub user_bazi: String,
    pub admin_chat_id: i64,
    pub database_url: String,
    pub expiration_minutes: i64,
    pub bazi_job_cron: String,
    pub context_cleanup_cron: String,
    pub log_cleanup_cron: String,
    pub log_retention_days: u64,
    pub max_context_messages: usize,
}

impl AppConfig {
    /// Load settings from environment variables and `.env` file.
    pub fn from_env() -> Self {
        // Load .env file
        dotenvy::dotenv().ok();

        let telegram_bot_token = env::var("TELEGRAM_BOT_TOKEN").expect("TELEGRAM_BOT_TOKEN must be set in .env");
        
        let openai_api_key =
            env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY must be set in .env");
        let openai_api_base = env::var("OPENAI_API_BASE").unwrap_or_default();
        let llm_model_name = env::var("LLM_MODEL_NAME").unwrap_or_else(|_| "gpt-4o".to_string());

        let user_bazi = env::var("USER_BAZI").unwrap_or_default();
        let admin_chat_id = env::var("ADMIN_CHAT_ID")
            .unwrap_or_else(|_| "0".to_string())
            .parse::<i64>()
            .unwrap_or(0);

        let database_url =
            env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://baziflow_agent.db".to_string());

        let expiration_minutes = env::var("EXPIRATION_MINUTES")
            .unwrap_or_else(|_| "30".to_string())
            .parse::<i64>()
            .unwrap_or(30);

        let bazi_job_cron = env::var("BAZI_JOB_CRON").unwrap_or_else(|_| "0 0 14 * * *".to_string());
        let context_cleanup_cron = env::var("CONTEXT_CLEANUP_CRON").unwrap_or_else(|_| "0 */5 * * * *".to_string());
        let log_cleanup_cron = env::var("LOG_CLEANUP_CRON").unwrap_or_else(|_| "0 0 3 * * *".to_string());
        let log_retention_days = env::var("LOG_RETENTION_DAYS")
            .unwrap_or_else(|_| "30".to_string())
            .parse::<u64>()
            .unwrap_or(30);

        let max_context_messages = env::var("MAX_CONTEXT_MESSAGES")
            .unwrap_or_else(|_| "10".to_string())
            .parse::<usize>()
            .unwrap_or(10);

        Self {
            telegram_bot_token,
            openai_api_key,
            openai_api_base,
            llm_model_name,
            user_bazi,
            admin_chat_id,
            database_url,
            expiration_minutes,
            bazi_job_cron,
            context_cleanup_cron,
            log_cleanup_cron,
            log_retention_days,
            max_context_messages,
        }
    }
}
