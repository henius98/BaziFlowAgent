use std::env;

/// Application configuration structure loaded from environment variables.
/// This can be used as a template for other projects to centralize configuration loading.
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub token: String,
    pub openai_api_key: String,
    pub openai_api_base: String,
    pub llm_model_name: String,
    pub user_bazi: String,
    pub admin_chat_id: i64,
    pub database_url: String,
}

impl AppConfig {
    /// Load settings from environment variables and `.env` file.
    pub fn from_env() -> Self {
        // Load .env file
        dotenvy::dotenv().ok();

        let token = env::var("TOKEN").expect("TOKEN must be set in .env");
        
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
            env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://dify_bot.db".to_string());

        Self {
            token,
            openai_api_key,
            openai_api_base,
            llm_model_name,
            user_bazi,
            admin_chat_id,
            database_url,
        }
    }
}
