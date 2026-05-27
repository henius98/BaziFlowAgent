use std::fs;
use std::time::{Duration, SystemTime};
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

/// Initializes the logging system for the application.
/// It uses the `RUST_LOG` environment variable if present.
/// It also outputs logs to the `logs/app.log` file with daily rotation.
///
/// Returns the `WorkerGuard` which MUST be held alive for the duration of the program.
pub fn init(config: &crate::config::AppConfig) -> (tracing_appender::non_blocking::WorkerGuard, tracing_appender::non_blocking::WorkerGuard) {
    // 1. Prepare the file appender (daily rotation, format: YYYY-MM-DD.log)
    let file_appender = tracing_appender::rolling::RollingFileAppender::builder()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("")
        .filename_suffix("log")
        .build("logs")
        .expect("initializing rolling file appender failed");
    let (file_non_blocking, file_guard) = tracing_appender::non_blocking(file_appender);

    // 2. Define the environment filter
    // If RUST_LOG is set, it takes precedence. Otherwise, use config.log_level.
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(format!("{},baziflow_agent={},hyper=info,reqwest=info,h2=info,mio=info", config.log_level, config.log_level)));

    // 3. Define the stdout layer (console) - NON-BLOCKING
    let (stdout_non_blocking, stdout_guard) = tracing_appender::non_blocking(std::io::stdout());
    let stdout_layer = fmt::layer().with_writer(stdout_non_blocking).compact().with_target(false);

    // 4. Define the file layer (no ANSI colors for the file)
    let file_layer = fmt::layer()
        .with_ansi(false)
        .with_writer(file_non_blocking)
        .with_file(true)
        .with_line_number(true)
        .with_thread_names(true);

    // 5. Initialize the registry with both layers
    tracing_subscriber::registry().with(env_filter).with(stdout_layer).with(file_layer).init();

    // 6. Post-initialization: Clean up logs older than configured days in a background thread
    let log_retention_days = config.log_retention_days;
    std::thread::spawn(move || {
        cleanup_old_logs(log_retention_days);
    });

    (file_guard, stdout_guard)
}

/// Automatically removes log files in the "logs" directory that are older than the specified number of days.
pub fn cleanup_old_logs(days: u64) {
    let log_dir = "logs";
    let now = SystemTime::now();
    let max_age = Duration::from_secs(days * 24 * 60 * 60);

    let Ok(entries) = fs::read_dir(log_dir) else { return };

    for entry in entries.flatten() {
        let path = entry.path();

        // Only target files ending in .log
        if !path.is_file() || path.extension().is_none_or(|ext| ext != "log") {
            continue;
        }

        let Ok(metadata) = entry.metadata() else { continue };
        let Ok(modified) = metadata.modified() else { continue };
        let Ok(age) = now.duration_since(modified) else { continue };

        if age > max_age {
            if let Err(e) = fs::remove_file(&path) {
                error!("Failed to delete old log file {:?}: {}", path, e);
            } else {
                info!("Automatically removed old log file: {:?}", path);
            }
        }
    }
}
