use chrono::Datelike;
use dashmap::DashMap;
use std::sync::Arc;
use teloxide::{prelude::*, utils::command::BotCommands};
use tracing::{error, info};

use crate::state::AppState;
use crate::calendar::{
    self, BirthdateCalAction, CalendarAction, TimePickerAction,
};
use crate::db;
use crate::llm_bazi;

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
            Some(&user.first_name),
            user.last_name.as_deref(),
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
            // Show birthdate calendar — fully keyboard-driven, no text input needed
            let now = chrono::Local::now();
            let markup = calendar::build_birthdate_calendar(now.year(), now.month());
            bot.send_message(msg.chat.id, "📅 Step 1/2 — Select your birthdate:\n\nNavigate with ◀️▶️ and tap a day.")
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
            BirthdateCalAction::SelectDate(date) => {
                let date_str = date.format("%Y-%m-%d").to_string();
                // Advance to hour picker
                let markup = calendar::build_hour_picker(&date_str);
                if let Some(msg) = &q.message {
                    // Use plain text — MarkdownV2 would require escaping hyphens in the date
                    let _ = bot
                        .edit_message_text(
                            msg.chat().id,
                            msg.id(),
                            format!("🕐 Step 2/2 — Select your birth hour for {}:", date_str),
                        )
                        .reply_markup(markup)
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

    // ── Time picker callbacks (bdtime:…) ──────────────────────────────────
    if calendar::is_time_picker_callback(data) {
        let action = match TimePickerAction::decode(data) {
            Some(a) => a,
            None => {
                bot.answer_callback_query(q.id).await?;
                return Ok(());
            }
        };

        match action {
            TimePickerAction::SelectHour { date, hour } => {
                // Show minute picker for the chosen hour
                let markup = calendar::build_minute_picker(&date, hour);
                if let Some(msg) = &q.message {
                    // Use plain text — MarkdownV2 would require escaping colons and hyphens
                    let _ = bot
                        .edit_message_text(
                            msg.chat().id,
                            msg.id(),
                            format!("🕐 Step 2/2 — Birth hour {:02}:__\nNow select the minute:", hour),
                        )
                        .reply_markup(markup)
                        .await;
                }
            }

            TimePickerAction::BackToHours { date } => {
                // Go back to hour picker
                let markup = calendar::build_hour_picker(&date);
                if let Some(msg) = &q.message {
                    // Use plain text — MarkdownV2 would require escaping hyphens in the date
                    let _ = bot
                        .edit_message_text(
                            msg.chat().id,
                            msg.id(),
                            format!("🕐 Step 2/2 — Select your birth hour for {}:", date),
                        )
                        .reply_markup(markup)
                        .await;
                }
            }

            TimePickerAction::SelectMinute { date, hour, minute } => {
                // All info collected — save to DB
                let user_id = q.from.id.0 as i64;
                let bazi_info = format!("出生日期：{} 出生时间：{:02}:{:02}", date, hour, minute);

                info!("Saving bazi for user {}: {}", user_id, bazi_info);
                db::save_or_update_user_bazi(&state.db_pool, user_id, &bazi_info).await;

                if let Some(msg) = &q.message {
                    let _ = bot
                        .edit_message_text(
                            msg.chat().id,
                            msg.id(),
                            format!(
                                "✅ Birth information saved!\n\n📅 Date: {}\n🕐 Time: {:02}:{:02}\n\nYour Bazi readings are now personalised. Use /start to begin your analysis.",
                                date, hour, minute
                            ),
                        )
                        .await;
                }
            }

            TimePickerAction::Ignore => {}
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
                Some(&user.first_name),
                user.last_name.as_deref(),
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
                let user_bazi = db::get_user_bazi(&state.db_pool, user_id)
                    .await
                    .unwrap_or_else(|| state.user_bazi.clone());

                match llm_bazi::generate_bazi_reading(
                    &state.http_client,
                    &formatted_date,
                    &ref_content,
                    &user_bazi,
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
                Some(&user.first_name),
                user.last_name.as_deref(),
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
                let user_bazi = db::get_user_bazi(&state.db_pool, user_id)
                    .await
                    .unwrap_or_else(|| state.user_bazi.clone());

                match llm_bazi::generate_bazi_reading(
                    &state.http_client,
                    &formatted_date,
                    &ref_content,
                    &user_bazi,
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

// ─────────────────────────────────────────────
// Message handler
// ─────────────────────────────────────────────

pub async fn handle_message(bot: Bot, msg: Message, state: Arc<AppState>) -> ResponseResult<()> {
    let text = match msg.text() {
        Some(t) if !t.starts_with('/') => t,
        _ => return Ok(()),
    };

    let user_id = msg.from.as_ref().map(|u| u.id.0 as i64).unwrap_or(0);
    if user_id == 0 {
        return Ok(());
    }

    if let Some(user) = msg.from.as_ref() {
        db::save_or_update_user(
            &state.db_pool,
            user_id,
            user.username.as_deref(),
            Some(&user.first_name),
            user.last_name.as_deref(),
        )
        .await;
    }

    // Performance optimization: cap context at 10 messages per user
    {
        let mut messages = state.user_contexts.entry(user_id).or_insert_with(Vec::new);
        if messages.len() >= 10 {
            messages.remove(0); // Keep max 10 messages in context
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
    let user_bazi = db::get_user_bazi(&state.db_pool, user_id)
        .await
        .unwrap_or_else(|| state.user_bazi.clone());

    let _ = bot.send_chat_action(msg.chat.id, teloxide::types::ChatAction::Typing).await;

    match llm_bazi::generate_bazi_reading(
        &state.http_client,
        &today,
        &ref_content,
        &user_bazi,
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
