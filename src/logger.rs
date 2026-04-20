use tracing::{error, info};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};
use std::fs;
use std::time::{Duration, SystemTime};

/// Initializes the logging system for the application.
/// It uses the `RUST_LOG` environment variable if present.
/// It also outputs logs to the `logs/app.log` file with daily rotation.
pub fn init() {
    // 1. Prepare the file appender (daily rotation, format: YYYY-MM-DD.log)
    let file_appender = tracing_appender::rolling::RollingFileAppender::builder()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("")
        .filename_suffix("log")
        .build("logs")
        .expect("initializing rolling file appender failed");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    // CRITICAL: We leak the guard so it stays alive for the duration of the program.
    // This ensures logs are flushed and the background thread keeps running.
    std::mem::forget(_guard);
    // 2. Define the environment filter
    // We look for LOG_LEVEL in .env, defaulting to "info"
    let log_level = std::env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string());
    
    let env_filter = EnvFilter::from_default_env()
        // Default level for the whole app
        .add_directive(log_level.parse().unwrap_or_else(|_| "info".parse().unwrap()))
        .add_directive(
            format!("baziflow_agent={}", log_level)
                .parse()
                .expect("Invalid log directive"),
        );
    // 3. Define the stdout layer (console)
    let stdout_layer = fmt::layer().with_writer(std::io::stdout);
    // 4. Define the file layer (no ANSI colors for the file)
    let file_layer = fmt::layer().with_ansi(false).with_writer(non_blocking);
    // 5. Initialize the registry with both layers
    tracing_subscriber::registry()
        .with(env_filter)
        .with(stdout_layer)
        .with(file_layer)
        .init();

    // 6. Post-initialization: Clean up logs older than 30 days
    cleanup_old_logs(30);
}

/// Automatically removes log files in the "logs" directory that are older than the specified number of days.
pub fn cleanup_old_logs(days: u64) {
    let log_dir = "logs";
    let now = SystemTime::now();
    let max_age = Duration::from_secs(days * 24 * 60 * 60);

    if let Ok(entries) = fs::read_dir(log_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            // Only target files ending in .log
            if path.is_file() && path.extension().map_or(false, |ext| ext == "log") {
                if let Ok(metadata) = entry.metadata() {
                    if let Ok(modified) = metadata.modified() {
                        if let Ok(age) = now.duration_since(modified) {
                            if age > max_age {
                                if let Err(e) = fs::remove_file(&path) {
                                    error!("Failed to delete old log file {:?}: {}", path, e);
                                } else {
                                    info!("Automatically removed old log file: {:?}", path);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
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
