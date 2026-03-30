mod api_extract;
mod calendar;
mod dify;
mod scheduler;

use chrono::{Datelike, Utc};
use dashmap::DashMap;
use std::sync::Arc;
use teloxide::{
    prelude::*,
    types::{Me, BotCommand},
    utils::command::BotCommands,
};
use tracing::{error, info};

/// Bot commands available in the menu
#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "Available commands:")]
enum Command {
    #[command(description = "Select a date for Bazi analysis")]
    Start,
}

/// Shared application state
struct AppState {
    http_client: reqwest::Client,
    webhook_url: String,
    /// Global dictionary to store user messages
    user_contexts: Arc<DashMap<i64, Vec<String>>>,
    /// Track when users were last active
    user_last_active: Arc<DashMap<i64, chrono::DateTime<Utc>>>,
}

/// Set expiration time (e.g., 30 minutes)
const EXPIRATION_MINUTES: i64 = 30;

#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("dify_telegram_bot=info".parse().unwrap()),
        )
        .init();

    // Load .env file
    dotenvy::dotenv().ok();

    // 1. Setup your Token from BotFather
    let token = std::env::var("TOKEN").expect("TOKEN must be set in .env");
    let webhook_url =
        std::env::var("DIFY_WEBHOOK_URL").expect("DIFY_WEBHOOK_URL must be set in .env");

    let bot = Bot::new(&token);
    let http_client = reqwest::Client::new();

    // Shared state
    let user_contexts: Arc<DashMap<i64, Vec<String>>> = Arc::new(DashMap::new());
    let user_last_active: Arc<DashMap<i64, chrono::DateTime<Utc>>> = Arc::new(DashMap::new());

    let state = Arc::new(AppState {
        http_client: http_client.clone(),
        webhook_url: webhook_url.clone(),
        user_contexts: user_contexts.clone(),
        user_last_active: user_last_active.clone(),
    });

    // Set bot commands in the menu
    if let Err(e) = bot
        .set_my_commands(vec![BotCommand::new("start", "Select Date")])
        .await
    {
        error!("Failed to set bot commands: {}", e);
    }

    // Initialize and start scheduler
    let scheduler_config = Arc::new(scheduler::SchedulerConfig {
        webhook_url: webhook_url.clone(),
        http_client: http_client.clone(),
    });

    let _scheduler = scheduler::start_scheduler(
        scheduler_config,
        user_contexts.clone(),
        user_last_active.clone(),
        EXPIRATION_MINUTES,
    )
    .await
    .expect("Failed to start scheduler");

    info!("Bot starting...");

    // Build the dispatcher with handlers
    let handler = dptree::entry()
        // Handle callback queries (calendar interactions)
        .branch(Update::filter_callback_query().endpoint(handle_callback))
        // Handle commands
        .branch(
            Update::filter_message()
                .filter_command::<Command>()
                .endpoint(handle_command),
        )
        // Handle regular messages (collecting user context)
        .branch(Update::filter_message().endpoint(handle_message));

    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![state])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

    info!("Bot stopped!");
}

/// 2. Command: /start (Sends the calendar)
async fn handle_command(bot: Bot, msg: Message, cmd: Command, state: Arc<AppState>) -> ResponseResult<()> {
    match cmd {
        Command::Start => {
            let now = chrono::Local::now();
            let markup =
                calendar::build_calendar(now.year(), now.month());
            bot.send_message(msg.chat.id, "Please select a date")
                .reply_markup(markup)
                .await?;
        }
    }
    Ok(())
}

/// 3. Callback: Handles clicking dates or switching months
async fn handle_callback(bot: Bot, q: CallbackQuery, state: Arc<AppState>) -> ResponseResult<()> {
    let data = match q.data.as_deref() {
        Some(d) => d,
        None => return Ok(()),
    };

    // Only handle calendar callbacks
    if !calendar::is_calendar_callback(data) {
        return Ok(());
    }

    let action = match calendar::CalendarAction::decode(data) {
        Some(a) => a,
        None => return Ok(()),
    };

    match action {
        calendar::CalendarAction::SelectDate(date) => {
            let formatted_date = date.format("%Y-%m-%d").to_string();
            let user_id = q.from.id.0 as i64;
            info!("User {} selected date: {}", user_id, formatted_date);

            // Edit the message to show processing
            if let Some(msg) = &q.message {
                let chat_id = msg.chat().id;
                let msg_id = msg.id();
                let _ = bot
                    .edit_message_text(chat_id, msg_id, format!("Processing date: {}", formatted_date))
                    .await;

                // Retrieve and format stored messages if any
                let ref_content = build_history_msg(&state.user_contexts, user_id);

                // Send to Dify
                match dify::send_to_dify(
                    &state.http_client,
                    &state.webhook_url,
                    &formatted_date,
                    &ref_content,
                )
                .await
                {
                    Ok(dify_response) => {
                        // Get Dify's answer
                        let result_text = dify::extract_dify_result(&dify_response);
                        bot.send_message(chat_id, format!("Dify received: {}", result_text))
                            .await?;
                    }
                    Err(e) => {
                        error!("Error: {}", e);
                        bot.send_message(chat_id, format!("Error connecting to Dify: {}", e))
                            .await?;
                    }
                }
            }

            // Stop loading animation
            bot.answer_callback_query(q.id).await?;
        }

        calendar::CalendarAction::Today => {
            let today = chrono::Local::now().date_naive();
            let formatted_date = today.format("%Y-%m-%d").to_string();
            let user_id = q.from.id.0 as i64;
            info!("User {} selected today: {}", user_id, formatted_date);

            if let Some(msg) = &q.message {
                let chat_id = msg.chat().id;
                let msg_id = msg.id();
                let _ = bot
                    .edit_message_text(chat_id, msg_id, format!("Processing date: {}", formatted_date))
                    .await;

                let ref_content = build_history_msg(&state.user_contexts, user_id);

                match dify::send_to_dify(
                    &state.http_client,
                    &state.webhook_url,
                    &formatted_date,
                    &ref_content,
                )
                .await
                {
                    Ok(dify_response) => {
                        let result_text = dify::extract_dify_result(&dify_response);
                        bot.send_message(chat_id, format!("Dify received: {}", result_text))
                            .await?;
                    }
                    Err(e) => {
                        error!("Error: {}", e);
                        bot.send_message(chat_id, format!("Error connecting to Dify: {}", e))
                            .await?;
                    }
                }
            }

            bot.answer_callback_query(q.id).await?;
        }

        calendar::CalendarAction::PrevMonth { year, month }
        | calendar::CalendarAction::NextMonth { year, month } => {
            // User is navigating months - update the calendar
            let markup = calendar::build_calendar(year, month);
            if let Some(msg) = &q.message {
                let _ = bot
                    .edit_message_reply_markup(msg.chat().id, msg.id())
                    .reply_markup(markup)
                    .await;
            }
            bot.answer_callback_query(q.id).await?;
        }

        calendar::CalendarAction::Ignore => {
            // Do nothing for header/empty cells
            bot.answer_callback_query(q.id).await?;
        }
    }

    Ok(())
}

/// 5. Handler for collecting user messages
async fn handle_message(bot: Bot, msg: Message, state: Arc<AppState>) -> ResponseResult<()> {
    let text = match msg.text() {
        Some(t) if !t.starts_with('/') => t,
        _ => return Ok(()),
    };

    let user_id = msg.from.as_ref().map(|u| u.id.0 as i64).unwrap_or(0);
    if user_id == 0 {
        return Ok(());
    }

    // Store message in context
    state
        .user_contexts
        .entry(user_id)
        .or_insert_with(Vec::new)
        .push(format!("User: {}", text));

    state.user_last_active.insert(user_id, Utc::now());
    info!("Stored message from {}: {}", user_id, text);

    // Send to Dify with today's date
    let today = chrono::Local::now()
        .date_naive()
        .format("%Y-%m-%d")
        .to_string();
    let ref_content = build_history_msg(&state.user_contexts, user_id);

    match dify::send_to_dify(&state.http_client, &state.webhook_url, &today, &ref_content).await {
        Ok(response) => {
            let result_text = dify::extract_dify_result(&response);
            bot.send_message(msg.chat.id, format!("Dify received: {}", result_text))
                .await?;
        }
        Err(e) => {
            error!("Error sending to Dify: {}", e);
            bot.send_message(msg.chat.id, format!("Error connecting to Dify: {}", e))
                .await?;
        }
    }

    Ok(())
}

/// Build history message string from user context
fn build_history_msg(user_contexts: &DashMap<i64, Vec<String>>, user_id: i64) -> String {
    // Retrieve and format stored messages if any
    if let Some(messages) = user_contexts.get(&user_id) {
        if !messages.is_empty() {
            return format!(
                "Here are the previous message:\n{}",
                messages.join("\n")
            );
        }
    }
    String::new()
}
