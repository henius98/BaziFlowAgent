use chrono::Datelike;
use dashmap::DashMap;
use std::sync::Arc;
use teloxide::{prelude::*, utils::command::BotCommands};
use tracing::{error, info};

use crate::state::AppState;
use crate::calendar::{
    self, BirthdateCalAction, CalendarAction, GenderAction,
};
use crate::db;
use crate::llm_bazi;
use crate::paipan;
// ─────────────────────────────────────────────
// Bot commands
// ─────────────────────────────────────────────

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "Available commands:")]
pub enum Command {
    #[command(description = "Select a date for Bazi analysis")]
    Start,
    #[command(description = "Set your birthdate & birth time for personalised readings")]
    New,
}

// ─────────────────────────────────────────────
// Command handler
// ─────────────────────────────────────────────

pub async fn handle_command(
    bot: Bot,
    msg: Message,
    cmd: Command,
    state: Arc<AppState>,
) -> ResponseResult<()> {
    if let Some(user) = msg.from.as_ref() {
        let user_id = user.id.0 as i64;
        db::save_or_update_user(
            &state.db_pool,
            user_id,
            user.username.as_deref(),
        )
        .await;

        db::save_request(
            &state.db_pool,
            user_id,
            "command",
            None,
            Some("/start"),
            None,
        )
        .await;
    }

    match cmd {
        Command::Start => {
            let now = chrono::Local::now();
            let markup = calendar::build_calendar(now.year(), now.month());
            bot.send_message(msg.chat.id, "Please select a date:")
                .reply_markup(markup)
                .await?;
        }

        Command::New => {
            let markup = calendar::build_gender_picker();
            bot.send_message(msg.chat.id, "📅 Step 1/5 — Select your gender:\n\nThis is required for accurate Bazi calculation.")
                .reply_markup(markup)
                .await?;
        }
    }
    Ok(())
}

// ─────────────────────────────────────────────
// Callback handler (calendar + time picker)
// ─────────────────────────────────────────────

pub async fn handle_callback(
    bot: Bot,
    q: CallbackQuery,
    state: Arc<AppState>,
) -> ResponseResult<()> {
    let data = match q.data.as_deref() {
        Some(d) => d,
        None => return Ok(()),
    };

    // ── Gender picker callbacks (bdgen:…) ──────────────────────────────────
    if calendar::is_gender_picker_callback(data) {
        let action = match GenderAction::decode(data) {
            Some(a) => a,
            None => {
                bot.answer_callback_query(q.id).await?;
                return Ok(());
            }
        };

        match action {
            GenderAction::SelectMale | GenderAction::SelectFemale => {
                let gender_val = if matches!(action, GenderAction::SelectMale) { 1 } else { 0 };
                let user_id = q.from.id.0 as i64;
                state.pending_gender.insert(user_id, gender_val);

                let markup = calendar::build_year_picker(1996);
                if let Some(msg) = &q.message {
                    let _ = bot
                        .edit_message_text(
                            msg.chat().id,
                            msg.id(),
                            "📅 Step 2/5 — Select your birth year:",
                        )
                        .reply_markup(markup)
                        .await;
                }
            }
            GenderAction::Ignore => {}
        }
        bot.answer_callback_query(q.id).await?;
        return Ok(());
    }

    // ── Birthdate calendar callbacks (bdcal:…) ────────────────────────────
    if calendar::is_birthdate_cal_callback(data) {
        let action = match BirthdateCalAction::decode(data) {
            Some(a) => a,
            None => {
                bot.answer_callback_query(q.id).await?;
                return Ok(());
            }
        };

        match action {
            BirthdateCalAction::ViewYears { start_year } => {
                let markup = calendar::build_year_picker(start_year);
                if let Some(msg) = &q.message {
                    let _ = bot.edit_message_reply_markup(msg.chat().id, msg.id())
                        .reply_markup(markup).await;
                }
            }
            BirthdateCalAction::SelectYear(year) => {
                let markup = calendar::build_month_picker(year);
                if let Some(msg) = &q.message {
                    let _ = bot.edit_message_text(
                        msg.chat().id,
                        msg.id(),
                        format!("📅 Step 3/5 — Year: {}\nNow select your birth month:", year)
                    ).reply_markup(markup).await;
                }
            }
            BirthdateCalAction::SelectMonth { year, month } => {
                let markup = calendar::build_birthdate_calendar(year, month);
                if let Some(msg) = &q.message {
                    let _ = bot.edit_message_text(
                        msg.chat().id,
                        msg.id(),
                        format!("📅 Step 4/5 — Year: {}, Month: {}\nNow select your birth day:", year, month)
                    ).reply_markup(markup).await;
                }
            }
            BirthdateCalAction::SelectDate(date) => {
                let date_str = date.format("%Y-%m-%d").to_string();
                let user_id = q.from.id.0 as i64;
                state.pending_birthdate.insert(user_id, date_str.clone());
                
                if let Some(msg) = &q.message {
                    let _ = bot
                        .edit_message_text(
                            msg.chat().id,
                            msg.id(),
                            format!("🕐 Step 5/5 — Enter birth time for {}:\n\nPlease type your birth time in **HH:MM** format (e.g., 14:30) and send it as a message.", date_str),
                        )
                        .await;
                }
            }
            BirthdateCalAction::PrevMonth { year, month }
            | BirthdateCalAction::NextMonth { year, month } => {
                let markup = calendar::build_birthdate_calendar(year, month);
                if let Some(msg) = &q.message {
                    let _ = bot
                        .edit_message_reply_markup(msg.chat().id, msg.id())
                        .reply_markup(markup)
                        .await;
                }
            }
            BirthdateCalAction::Ignore => {}
        }

        bot.answer_callback_query(q.id).await?;
        return Ok(());
    }


    // ── Bazi analysis calendar callbacks (cal:…) ─────────────────────────
    if !calendar::is_calendar_callback(data) {
        return Ok(());
    }

    let action = match CalendarAction::decode(data) {
        Some(a) => a,
        None => return Ok(()),
    };

    // Answer the callback query immediately to stop the loading spinner on the button
    // BEFORE starting the long LLM generation process.
    let _ = bot.answer_callback_query(q.id).await;

    match action {
        CalendarAction::SelectDate(date) => {
            let formatted_date = date.format("%Y-%m-%d").to_string();
            let user = &q.from;
            let user_id = user.id.0 as i64;
            info!("User {} selected date: {}", user_id, formatted_date);

            db::save_or_update_user(
                &state.db_pool,
                user_id,
                user.username.as_deref(),
            )
            .await;

            if let Some(msg) = &q.message {
                let chat_id = msg.chat().id;
                let msg_id = msg.id();
                let _ = bot
                    .edit_message_text(
                        chat_id,
                        msg_id,
                        format!("Processing date: {}", formatted_date),
                    )
                    .await;

                let ref_content = build_history_msg(&state.user_contexts, user_id);
                let (user_profile_bazi, destiny_reading) = db::get_user_profile(&state.db_pool, user_id).await;
                let user_bazi_raw = user_profile_bazi.unwrap_or_else(|| state.user_bazi.clone());
                let user_bazi = get_formatted_bazi(&user_bazi_raw);
                let destiny_reading = destiny_reading.unwrap_or_default();

                match llm_bazi::generate_bazi_reading(
                    &state.http_client,
                    &formatted_date,
                    &ref_content,
                    &user_bazi,
                    &destiny_reading,
                    &state.openai_api_key,
                    &state.openai_api_base,
                    &state.llm_model_name,
                )
                .await
                {
                    Ok(result_text) => {
                        db::save_request(
                            &state.db_pool,
                            user_id,
                            "calendar_date",
                            Some(&formatted_date),
                            Some(&ref_content),
                            Some(&result_text),
                        )
                        .await;
                        bot.send_message(chat_id, format!("📝 盲派命理分析：\n{}", result_text))
                            .await?;
                    }
                    Err(e) => {
                        error!("Error: {}", e);
                        db::save_request(
                            &state.db_pool,
                            user_id,
                            "calendar_date",
                            Some(&formatted_date),
                            Some(&ref_content),
                            Some(&format!("Error: {}", e)),
                        )
                        .await;
                        bot.send_message(chat_id, format!("Error generating reading: {}", e))
                            .await?;
                    }
                }
            }
        }

        CalendarAction::Today => {
            let today = chrono::Local::now().date_naive();
            let formatted_date = today.format("%Y-%m-%d").to_string();
            let user = &q.from;
            let user_id = user.id.0 as i64;
            info!("User {} selected today: {}", user_id, formatted_date);

            db::save_or_update_user(
                &state.db_pool,
                user_id,
                user.username.as_deref(),
            )
            .await;

            if let Some(msg) = &q.message {
                let chat_id = msg.chat().id;
                let msg_id = msg.id();
                let _ = bot
                    .edit_message_text(
                        chat_id,
                        msg_id,
                        format!("Processing date: {}", formatted_date),
                    )
                    .await;

                let ref_content = build_history_msg(&state.user_contexts, user_id);
                let (user_profile_bazi, destiny_reading) = db::get_user_profile(&state.db_pool, user_id).await;
                let user_bazi_raw = user_profile_bazi.unwrap_or_else(|| state.user_bazi.clone());
                let user_bazi = get_formatted_bazi(&user_bazi_raw);
                let destiny_reading = destiny_reading.unwrap_or_default();

                match llm_bazi::generate_bazi_reading(
                    &state.http_client,
                    &formatted_date,
                    &ref_content,
                    &user_bazi,
                    &destiny_reading,
                    &state.openai_api_key,
                    &state.openai_api_base,
                    &state.llm_model_name,
                )
                .await
                {
                    Ok(result_text) => {
                        db::save_request(
                            &state.db_pool,
                            user_id,
                            "calendar_today",
                            Some(&formatted_date),
                            Some(&ref_content),
                            Some(&result_text),
                        )
                        .await;
                        bot.send_message(chat_id, format!("📝 今日盲派分析：\n{}", result_text))
                            .await?;
                    }
                    Err(e) => {
                        error!("Error: {}", e);
                        db::save_request(
                            &state.db_pool,
                            user_id,
                            "calendar_today",
                            Some(&formatted_date),
                            Some(&ref_content),
                            Some(&format!("Error: {}", e)),
                        )
                        .await;
                        bot.send_message(chat_id, format!("Error generating reading: {}", e))
                            .await?;
                    }
                }
            }
        }

        CalendarAction::PrevMonth { year, month } | CalendarAction::NextMonth { year, month } => {
            let markup = calendar::build_calendar(year, month);
            if let Some(msg) = &q.message {
                let _ = bot
                    .edit_message_reply_markup(msg.chat().id, msg.id())
                    .reply_markup(markup)
                    .await;
            }
        }

        CalendarAction::Ignore => {}
    }

    Ok(())
}

/// Core logic for Bazi chart calculation and destiny reading generation.
pub async fn perform_bazi_analysis(
    state: Arc<AppState>,
    bot: Bot,
    chat_id: ChatId,
    user_id: i64,
    hour: u32,
    minute: u32,
) -> ResponseResult<()> {
    let date = state.pending_birthdate.get(&user_id)
        .map(|v| v.clone())
        .unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d").to_string());
    
    let gender = state.pending_gender.get(&user_id).map(|v| *v).unwrap_or(1);

    // Let the user know we're working
    let _ = bot.send_message(
        chat_id, 
        format!("✅ Time received.\n⏳ Calculating your Bazi chart...\n\n📅 Date: {}\n🕐 Time: {:02}:{:02}", date, hour, minute),
    ).await;

    let _ = bot.send_chat_action(chat_id, teloxide::types::ChatAction::Typing).await;

    match paipan::fetch_bazi_chart(&state.http_client, &date, hour, minute, gender).await {
        Ok((chart, raw_json)) => {
            let birth_dt_str = format!("{} {:02}:{:02}:00", date, hour, minute);
            db::save_or_update_user_bazi(&state.db_pool, user_id, &raw_json, gender, Some(&birth_dt_str)).await;
            let formatted_bazi = paipan::format_bazi_for_prompt(&chart);

            let _ = bot.send_message(
                chat_id,
                "✅ Bazi chart calculated!\n🔮 Generating destiny analysis... (this may take a moment)",
            ).await;

            match llm_bazi::generate_destiny_reading(
                &formatted_bazi,
                &state.openai_api_key,
                &state.openai_api_base,
                &state.llm_model_name,
            ).await {
                Ok(reading) => {
                    db::save_user_destiny_reading(&state.db_pool, user_id, &reading).await;

                    db::save_request(
                        &state.db_pool,
                        user_id,
                        "new_bazi_reading",
                        Some(&date),
                        Some(&format!("Birth details updated (Gender: {})", gender)),
                        Some(&reading),
                    ).await;

                    let parts = split_message(&reading, 4000);
                    for part in parts {
                        let _ = bot.send_message(chat_id, part).await;
                    }
                }
                Err(e) => {
                    error!("Error generating destiny reading: {}", e);
                    let _ = bot.send_message(chat_id, format!("❌ Error generating analysis: {}", e)).await;
                }
            }
        }
        Err(e) => {
            error!("Failed to fetch bazi chart: {}", e);
            let _ = bot.send_message(chat_id, format!("❌ Error fetching Bazi chart from API. Please try again later.")).await;
        }
    }
    
    // Clean up pending states
    state.pending_birthdate.remove(&user_id);
    state.pending_gender.remove(&user_id);
    
    Ok(())
}

// ─────────────────────────────────────────────
// Message handler
// ─────────────────────────────────────────────

pub async fn handle_message(bot: Bot, msg: Message, state: Arc<AppState>) -> ResponseResult<()> {
    let user_id = msg.from.as_ref().map(|u| u.id.0 as i64).unwrap_or(0);
    if user_id == 0 {
        return Ok(());
    }

    if let Some(user) = msg.from.as_ref() {
        db::save_or_update_user(
            &state.db_pool,
            user_id,
            user.username.as_deref(),
        )
        .await;
    }


    let text = match msg.text() {
        Some(t) if !t.starts_with('/') => t,
        _ => return Ok(()),
    };

    // If user has a pending birthdate, check if they are providing the time via text
    if state.pending_birthdate.contains_key(&user_id) {
        let parts: Vec<&str> = text.split(':').collect();
        if parts.len() == 2 {
            if let (Ok(hour), Ok(minute)) = (parts[0].trim().parse::<u32>(), parts[1].trim().parse::<u32>()) {
                if hour < 24 && minute < 60 {
                    let chat_id = msg.chat.id;
                    let state_clone = state.clone();
                    let bot_clone = bot.clone();
                    tokio::spawn(async move {
                        let _ = perform_bazi_analysis(state_clone, bot_clone, chat_id, user_id, hour, minute).await;
                    });
                    return Ok(());
                }
            }
        }
    }

    // Performance optimization: cap context at max_context_messages per user
    {
        let mut messages = state.user_contexts.entry(user_id).or_insert_with(Vec::new);
        if messages.len() >= state.max_context_messages {
            messages.remove(0); // Keep max messages in context
        }
        messages.push(format!("User: {}", text));
    }

    state.user_last_active.insert(user_id, chrono::Utc::now());
    info!("Stored message from {}: {}", user_id, text);

    let today = chrono::Local::now()
        .date_naive()
        .format("%Y-%m-%d")
        .to_string();
    let ref_content = build_history_msg(&state.user_contexts, user_id);
    let (user_bazi_raw, destiny_reading) = db::get_user_profile(&state.db_pool, user_id).await;
    let user_bazi_raw = user_bazi_raw.unwrap_or_else(|| state.user_bazi.clone());
    let user_bazi = get_formatted_bazi(&user_bazi_raw);
    let destiny_reading = destiny_reading.unwrap_or_default();

    let _ = bot.send_chat_action(msg.chat.id, teloxide::types::ChatAction::Typing).await;

    match llm_bazi::generate_bazi_reading(
        &state.http_client,
        &today,
        &ref_content,
        &user_bazi,
        &destiny_reading,
        &state.openai_api_key,
        &state.openai_api_base,
        &state.llm_model_name,
    )
    .await
    {
        Ok(result_text) => {
            db::save_request(
                &state.db_pool,
                user_id,
                "message",
                Some(&today),
                Some(text),
                Some(&result_text),
            )
            .await;
            bot.send_message(msg.chat.id, format!("📝 回复：\n{}", result_text))
                .await?;
        }
        Err(e) => {
            error!("Error generating reading: {}", e);
            db::save_request(
                &state.db_pool,
                user_id,
                "message",
                Some(&today),
                Some(text),
                Some(&format!("Error: {}", e)),
            )
            .await;
            bot.send_message(msg.chat.id, format!("Error processing request: {}", e))
                .await?;
        }
    }

    Ok(())
}

// ─────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────

fn build_history_msg(user_contexts: &DashMap<i64, Vec<String>>, user_id: i64) -> String {
    if let Some(messages) = user_contexts.get(&user_id) {
        if !messages.is_empty() {
            return format!("Here are the previous message:\n{}", messages.join("\n"));
        }
    }
    String::new()
}

fn get_formatted_bazi(raw_db_str: &str) -> String {
    if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(raw_db_str) {
        if let Ok(chart) = serde_json::from_value::<crate::paipan::BaziChart>(json_val) {
            return crate::paipan::format_bazi_for_prompt(&chart);
        }
    }
    raw_db_str.to_string()
}

fn split_message(text: &str, limit: usize) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    
    for line in text.lines() {
        if current.len() + line.len() > limit {
            result.push(current.clone());
            current.clear();
        }
        current.push_str(line);
        current.push('\n');
    }
    
    if !current.is_empty() {
        result.push(current);
    }
    
    if result.is_empty() {
        result.push(text.to_string());
    }
    
    result
}
