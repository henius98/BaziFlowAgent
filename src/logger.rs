use tracing::error;
use tracing_subscriber::{fmt, EnvFilter};

/// Initializes the logging system for the application.
/// It uses the `RUST_LOG` environment variable if present.
/// By default, it sets the log level for `dify_telegram_bot` crate to `info`.
pub fn init() {
    fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive("dify_telegram_bot=info".parse().expect("Invalid log directive")),
        )
        .init();
}

/// A unified error type for the application.
/// It wraps various system libraries and automatically logs errors when they happen.
#[derive(Debug)]
pub enum AppError {
    /// HTTP exceptions (e.g. timeout, connection reset, 404/500 responses)
    Http(reqwest::Error),
    /// Data serialization or deserialization issues
    Json(serde_json::Error),
    /// Database exceptions
    Db(sqlx::Error),
    /// OpenAI API exceptions
    OpenAI(async_openai::error::OpenAIError),
    /// Wrapper for anyhow/system level fallback
    System(anyhow::Error),
    /// Generic string-based custom messages
    Message(String),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(e) => write!(f, "HTTP Error: {}", e),
            Self::Json(e) => write!(f, "JSON Error: {}", e),
            Self::Db(e) => write!(f, "Database Error: {}", e),
            Self::OpenAI(e) => write!(f, "OpenAI API Error: {}", e),
            Self::System(e) => write!(f, "System Error: {}", e),
            Self::Message(e) => write!(f, "Application Error: {}", e),
        }
    }
}

impl std::error::Error for AppError {}

// Auto-conversion traits that connect to the logging module gracefully

impl From<reqwest::Error> for AppError {
    fn from(err: reqwest::Error) -> Self {
        error!("HTTP Exception occurred: {}", err);
        Self::Http(err)
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        error!("JSON Parsing Exception occurred: {}", err);
        Self::Json(err)
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        error!("Database Exception occurred: {}", err);
        Self::Db(err)
    }
}

impl From<async_openai::error::OpenAIError> for AppError {
    fn from(err: async_openai::error::OpenAIError) -> Self {
        error!("OpenAI API Exception occurred: {}", err);
        Self::OpenAI(err)
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        error!("System Exception occurred: {}", err);
        Self::System(err)
    }
}

impl AppError {
    /// Helper for simple string errors
    pub fn context(msg: impl Into<String>) -> Self {
        let msg = msg.into();
        error!("Explicit Application Exception: {}", msg);
        Self::Message(msg)
    }
}

/// Helper extension trait to log and map specific Results quickly
pub trait LogErrorExt<T> {
    fn log_err_msg(self, context_msg: &str) -> Result<T, AppError>;
}

impl<T, E> LogErrorExt<T> for Result<T, E>
where
    E: Into<AppError>,
{
    fn log_err_msg(self, context_msg: &str) -> Result<T, AppError> {
        self.map_err(|e| {
            let app_err: AppError = e.into();
            error!("Context Failed - {}: {}", context_msg, app_err);
            app_err
        })
    }
}

pub type AppResult<T> = Result<T, AppError>;
