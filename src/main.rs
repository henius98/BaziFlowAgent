mod almanac;
mod calendar;
mod config;
mod db;
mod handlers;
mod llm_bazi;
mod logger;
mod paipan;
mod scheduler;
mod state;

use handlers::Command;
use std::sync::Arc;
use teloxide::{prelude::*, types::BotCommand};
use tracing::{error, info};

use config::AppConfig;
use state::AppState;

#[tokio::main]
async fn main() {
    // Initialize logging
    logger::init();
    
    let config = AppConfig::from_env();

    let bot = Bot::new(&config.telegram_bot_token);
    // Set a custom User-Agent since some webhooks/Cloudflare block default bot UAs
    let http_client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    // Initialize database
    let db_pool = db::init_db(&config.database_url)
        .await
        .expect("Failed to initialize database");

    // Shared state
    let state = Arc::new(AppState::new(
        http_client.clone(),
        db_pool,
        config.openai_api_key,
        config.openai_api_base,
        config.llm_model_name,
        config.user_bazi,
        config.max_context_messages,
    ));

    if let Err(e) = bot
        .set_my_commands(vec![
            BotCommand::new("start", "Select Date"),
            BotCommand::new("new", "Set your birthdate bazi"),
        ])
        .await
    {
        error!("Failed to set bot commands: {}", e);
    }

    // Initialize and start scheduler
    let scheduler_config = Arc::new(scheduler::SchedulerConfig {
        http_client: http_client.clone(),
        bot: bot.clone(),
        app_state: state.clone(),
        admin_chat_id: config.admin_chat_id,
        bazi_job_cron: config.bazi_job_cron,
        context_cleanup_cron: config.context_cleanup_cron,
        log_cleanup_cron: config.log_cleanup_cron,
        log_retention_days: config.log_retention_days,
    });

    let _scheduler = scheduler::start_scheduler(
        scheduler_config,
        state.user_contexts.clone(),
        state.user_last_active.clone(),
        config.expiration_minutes,
    )
    .await
    .expect("Failed to start scheduler");

    info!("BaziFlowAgent starting...");

    // Build the dispatcher with handlers
    let handler = dptree::entry()
        // Handle callback queries (calendar interactions)
        .branch(Update::filter_callback_query().endpoint(handlers::handle_callback))
        // Handle commands
        .branch(
            Update::filter_message()
                .filter_command::<Command>()
                .endpoint(handlers::handle_command),
        )
        // Handle regular messages (collecting user context)
        .branch(Update::filter_message().endpoint(handlers::handle_message));

    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![state])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

    info!("Bot stopped!");
}
