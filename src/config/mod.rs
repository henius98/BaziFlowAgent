use std::env;

/// Application configuration structure loaded from environment variables.
#[derive(Debug)]
pub struct AppConfig {
    pub telegram_bot_token: String,
    pub openai_api_key: String,
    pub openai_api_base: String,
    pub llm_model_name: String,
    pub database_url: String,
    pub user_contexts_expiration_minutes: i64,
    pub bazi_job_cron: String,
    pub context_cleanup_cron: String,
    pub log_cleanup_cron: String,
    pub log_retention_days: u64,
    pub max_context_messages: usize,
    pub base_url: String,
    pub log_level: String,
}

impl AppConfig {
    /// Load settings from environment variables and `.env` file.
    pub fn from_env() -> anyhow::Result<Self> {
        use anyhow::Context;

        // Load .env file
        dotenvy::dotenv().ok();

        let telegram_bot_token = env::var("TELEGRAM_BOT_TOKEN").context("TELEGRAM_BOT_TOKEN must be set in .env").and_then(|t| {
            if t.trim().is_empty() {
                anyhow::bail!("TELEGRAM_BOT_TOKEN is invalid or contains the default placeholder");
            }
            Ok(t)
        })?;

        let openai_api_key = env::var("OPENAI_API_KEY").context("OPENAI_API_KEY must be set in .env")?;
        let openai_api_base = env::var("OPENAI_API_BASE").context("OPENAI_API_BASE must be set in .env")?;
        let llm_model_name = env::var("LLM_MODEL_NAME").context("LLM_MODEL_NAME must be set in .env")?;

        let database_url = env::var("DATABASE_URL").context("DATABASE_URL must be set in .env")?;

        let user_contexts_expiration_minutes = env::var("USER_CONTEXTS_EXPIRATION_MINUTES")
            .context("USER_CONTEXTS_EXPIRATION_MINUTES must be set in .env")?
            .parse::<i64>()
            .context("USER_CONTEXTS_EXPIRATION_MINUTES must be a valid i64")?;

        let bazi_job_cron = env::var("BAZI_JOB_CRON").context("BAZI_JOB_CRON must be set in .env")?;
        let context_cleanup_cron = env::var("CONTEXT_CLEANUP_CRON").context("CONTEXT_CLEANUP_CRON must be set in .env")?;
        let log_cleanup_cron = env::var("LOG_CLEANUP_CRON").context("LOG_CLEANUP_CRON must be set in .env")?;
        let log_retention_days = env::var("LOG_RETENTION_DAYS")
            .context("LOG_RETENTION_DAYS must be set in .env")?
            .parse::<u64>()
            .context("LOG_RETENTION_DAYS must be a valid u64")?;

        let max_context_messages = env::var("MAX_CONTEXT_MESSAGES")
            .context("MAX_CONTEXT_MESSAGES must be set in .env")?
            .parse::<usize>()
            .context("MAX_CONTEXT_MESSAGES must be a valid usize")?;

        let base_url = env::var("BASE_URL").context("BASE_URL must be set in .env")?;

        let log_level = env::var("LOG_LEVEL").context("LOG_LEVEL must be set in .env")?;

        Ok(Self {
            telegram_bot_token,
            openai_api_key,
            openai_api_base,
            llm_model_name,
            database_url,
            user_contexts_expiration_minutes,
            bazi_job_cron,
            context_cleanup_cron,
            log_cleanup_cron,
            log_retention_days,
            max_context_messages,
            base_url,
            log_level,
        })
    }
}
