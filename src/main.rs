use std::sync::Arc;
use teloxide::{prelude::*, utils::command::BotCommands};
use tracing::{debug, error, info};

use baziflow_agent::bot;
use baziflow_agent::config::AppConfig;
use baziflow_agent::logger;
use baziflow_agent::models::AppState;
use baziflow_agent::repos;
use baziflow_agent::scheduler;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use anyhow::Context;

    let config = AppConfig::from_env().context("Failed to load configuration")?;

    // Initialize logging — _log_guard must live for the duration of main()
    let _log_guard = logger::init(&config).context("Failed to initialize logger")?;

    let config = Arc::new(config);

    let bot = Bot::new(&config.telegram_bot_token);

    // Initialize database
    let db_pool = repos::init_db(&config.database_url).await.context("Failed to initialize database")?;

    // Set a custom User-Agent since some webhooks/Cloudflare block default bot UAs
    let http_client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    // Shared state
    let state = Arc::new(AppState::new(http_client.clone(), db_pool, config.clone()));
    baziflow_agent::models::state::GLOBAL_STATE.set(state).map_err(|_| anyhow::anyhow!("Failed to set GLOBAL_STATE"))?;

    // Set bot commands
    bot.set_my_commands(bot::Command::bot_commands()).await.context("Failed to set bot commands")?;

    // Initialize and start scheduler
    let scheduler_config = Arc::new(scheduler::SchedulerConfig { bot: bot.clone() });

    let _scheduler = scheduler::start_scheduler(scheduler_config, config.user_contexts_expiration_minutes)
        .await
        .map_err(|e| anyhow::anyhow!(e))
        .context("Failed to start scheduler")?;
    debug!("BaziFlowAgent starting services...");

    // 1. Build the Telegram bot dispatcher
    let handler = dptree::entry()
        .branch(Update::filter_callback_query().endpoint(bot::callbacks::handle_callback))
        .branch(Update::filter_message().filter_command::<bot::Command>().endpoint(bot::commands::handle_command))
        .branch(Update::filter_message().endpoint(bot::messages::handle_message));

    let mut bot_dispatcher = Dispatcher::builder(bot.clone(), handler).build();

    let ctrl_c = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            error!("Failed to listen for ctrl_c signal: {}", e);
        } else {
            info!("Shutdown signal received");
        }
    };

    // Ensure public directory exists for static charts
    let _ = tokio::fs::create_dir_all("public").await;

    // Start minimal web server for Instant View / Web view
    let app = axum::Router::new().fallback_service(tower_http::services::ServeDir::new("public"));

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], 8080));
    info!("Starting web server on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.context("Failed to bind web server")?;
    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            error!("Web server error: {}", e);
        }
    });

    tokio::select! {
        _ = bot_dispatcher.dispatch() => {
            info!("Bot dispatcher stopped");
        }
        _ = ctrl_c => {
            info!("Graceful shutdown triggered");
        }
    }

    info!("BaziFlowAgent stopped!");
    Ok(())
}
