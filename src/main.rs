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

  let (bot, telegram_worker) = teloxide::adaptors::Throttle::new(Bot::new(&config.telegram_bot_token), Default::default());
  let telegram_worker = tokio::spawn(telegram_worker);

  // Initialize database
  let db_pool = repos::init_db(&config.database_url).await.context("Failed to initialize database")?;
  let chat_cache = repos::chat_history_cache::init_chat_history_cache(&config.chat_cache_database_url, config.chat_cache_max_messages).await.context("Failed to initialize chat history cache")?;

  // Set a custom User-Agent since some webhooks/Cloudflare block default bot UAs
  let http_client = reqwest::Client::builder()
    .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
    .connect_timeout(std::time::Duration::from_secs(5))
    .timeout(std::time::Duration::from_secs(30))
    .build()
    .context("Failed to build HTTP client")?;

  // Shared state
  let state = Arc::new(AppState::new(http_client, db_pool, chat_cache.pool, chat_cache.writer, config.clone())?);
  baziflow_agent::models::state::GLOBAL_STATE.set(state.clone()).map_err(|_| anyhow::anyhow!("Failed to set GLOBAL_STATE"))?;

  // Set bot commands
  bot.set_my_commands(bot::Command::bot_commands()).await.context("Failed to set bot commands")?;

  // Initialize and start scheduler
  let scheduler_config = Arc::new(scheduler::SchedulerConfig { bot: bot.clone() });

  let mut scheduler = scheduler::start_scheduler(scheduler_config, config.user_contexts_expiration_minutes).await.map_err(|e| anyhow::anyhow!(e)).context("Failed to start scheduler")?;
  debug!("BaziFlowAgent starting services...");

  // 1. Build the Telegram bot dispatcher
  let handler = dptree::entry()
    .branch(Update::filter_callback_query().endpoint(|bot: bot::Bot, q: CallbackQuery| async move { bot::run(bot::callbacks::handle_callback(bot, q)).await }))
    .branch(
      Update::filter_message().filter_command::<bot::Command>().endpoint(|bot: bot::Bot, msg: Message, cmd: bot::Command| async move { bot::run(bot::commands::handle_command(bot, msg, cmd)).await }),
    )
    .branch(Update::filter_message().endpoint(|bot: bot::Bot, msg: Message| async move { bot::run(bot::messages::handle_message(bot, msg)).await }));

  let mut bot_dispatcher = Dispatcher::builder(bot.clone(), handler).build();

  // Ensure public directory exists for static charts
  let _ = tokio::fs::create_dir_all("public").await;

  // Start web server with API routes + static file serving for Instant View / Web view
  let app = baziflow_agent::api::api_router(config.clone()).route("/charts/{token}", axum::routing::get(baziflow_agent::api::charts::chart));

  let addr = config.runtime.bind;
  info!("Starting web server on http://{}", addr);

  let listener = tokio::net::TcpListener::bind(addr).await.context("Failed to bind web server")?;
  let shutdown = state.runtime.shutdown.clone();
  let mut web_task = tokio::spawn(async move { axum::serve(baziflow_agent::api::listener(listener), app).with_graceful_shutdown(shutdown.cancelled_owned()).await });
  let dispatcher_shutdown = bot_dispatcher.shutdown_token();
  let mut bot_task = tokio::spawn(async move { bot_dispatcher.dispatch().await });
  tokio::select! {
    result = &mut web_task => { error!(?result, "Web server stopped"); }
    result = &mut bot_task => { info!(?result, "Bot dispatcher stopped"); }
    result = shutdown_signal() => { result?; info!("Shutdown signal received"); }
  }
  state.runtime.shutdown.cancel();
  let _ = dispatcher_shutdown.shutdown();
  let drain = async {
    if let Err(error) = scheduler.shutdown().await {
      error!(%error, "Scheduler shutdown failed");
    }
    if !bot_task.is_finished() {
      let _ = (&mut bot_task).await;
    }
    if !web_task.is_finished() {
      let _ = (&mut web_task).await;
    }
    state.runtime.tasks.close();
    state.runtime.tasks.wait().await;
    let _ = state.chat_cache_writer.flush().await;
    state.chat_cache_pool.close().await;
    state.db_pool.close().await;
  };
  if tokio::time::timeout(std::time::Duration::from_secs(config.runtime.shutdown_seconds), drain).await.is_err() {
    error!("Shutdown drain deadline exceeded");
    bot_task.abort();
    web_task.abort();
  }
  telegram_worker.abort();
  info!("BaziFlowAgent stopped!");
  Ok(())
}

async fn shutdown_signal() -> std::io::Result<()> {
  #[cfg(unix)]
  {
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    tokio::select! { result = tokio::signal::ctrl_c() => result, _ = terminate.recv() => Ok(()) }
  }
  #[cfg(not(unix))]
  tokio::signal::ctrl_c().await
}
