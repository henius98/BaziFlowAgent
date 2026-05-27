use thiserror::Error;
use tracing::error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("HTTP Error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON Error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Database Error: {0}")]
    Db(#[from] sqlx::Error),

    #[error("OpenAI API Error: {0}")]
    OpenAI(#[from] async_openai::error::OpenAIError),

    #[error("System Error: {0}")]
    System(#[from] anyhow::Error),

    #[error("Telegram Error: {0}")]
    Telegram(#[from] teloxide::RequestError),

    #[error("Application Error: {0}")]
    Message(String),
}

impl AppError {
    pub fn context(msg: impl Into<String>) -> Self {
        Self::Message(msg.into())
    }
}

impl From<&str> for AppError {
    fn from(msg: &str) -> Self {
        AppError::Message(msg.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

/// Helper extension trait to log and map specific Results quickly
pub trait LogErrorExt<T> {
    fn log_err_msg(self, context_msg: &str) -> AppResult<T>;
}

impl<T, E> LogErrorExt<T> for Result<T, E>
where
    E: Into<AppError>,
{
    fn log_err_msg(self, context_msg: &str) -> AppResult<T> {
        self.map_err(|e| {
            let app_err: AppError = e.into();
            error!("{}: {}", context_msg, app_err);
            app_err
        })
    }
}
