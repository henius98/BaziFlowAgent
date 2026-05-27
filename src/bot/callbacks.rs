use std::sync::Arc;
use teloxide::prelude::*;
use tracing::{error, info};

use super::bazi_flow;
use super::calendar::{self, BirthdateCalAction, CalendarAction, GenderAction, LocationAction, TimeAction};
use super::helpers::{build_history_msg, get_display_name};
use crate::models::AppState;
use crate::repos;
use crate::services::llm_bazi;
use crate::utils;

// ─────────────────────────────────────────────
// Callback handler (calendar + time picker)
// ─────────────────────────────────────────────

pub async fn handle_callback(bot: Bot, q: CallbackQuery, state: Arc<AppState>) -> ResponseResult<()> {
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
                let user_id = q.from.id.0;
                state.user_contexts.entry(user_id).or_default().gender = Some(gender_val);

                let markup = calendar::build_year_picker(1996);
                if let Some(msg) = &q.message {
                    let _ = bot.edit_message_text(msg.chat().id, msg.id(), "📅 Step 2/6 — Select your birth year:").reply_markup(markup).await;
                }
            }
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
                    let _ = bot.edit_message_reply_markup(msg.chat().id, msg.id()).reply_markup(markup).await;
                }
            }
            BirthdateCalAction::SelectYear(year) => {
                let markup = calendar::build_month_picker(year);
                if let Some(msg) = &q.message {
                    let _ = bot
                        .edit_message_text(msg.chat().id, msg.id(), format!("📅 Step 3/6 — Year: {}\nNow select your birth month:", year))
                        .reply_markup(markup)
                        .await;
                }
            }
            BirthdateCalAction::SelectMonth { year, month } => {
                let markup = calendar::build_birthdate_calendar(year, month);
                if let Some(msg) = &q.message {
                    let _ = bot
                        .edit_message_text(msg.chat().id, msg.id(), format!("📅 Step 4/6 — Year: {}, Month: {}\nNow select your birth day:", year, month))
                        .reply_markup(markup)
                        .await;
                }
            }
            BirthdateCalAction::SelectDate(date) => {
                let date_str = date.format("%Y-%m-%d").to_string();
                let user_id = q.from.id.0;
                state.user_contexts.entry(user_id).or_default().birthdate = Some(date_str.clone());

                let markup = calendar::build_hour_picker();
                if let Some(msg) = &q.message {
                    let _ = bot
                        .edit_message_text(msg.chat().id, msg.id(), format!("🕐 Step 5/6 — Select birth hour for {}:", date_str))
                        .reply_markup(markup)
                        .await;
                }
            }
            BirthdateCalAction::PrevMonth { year, month } | BirthdateCalAction::NextMonth { year, month } => {
                let markup = calendar::build_birthdate_calendar(year, month);
                if let Some(msg) = &q.message {
                    let _ = bot.edit_message_reply_markup(msg.chat().id, msg.id()).reply_markup(markup).await;
                }
            }
        }
        bot.answer_callback_query(q.id).await?;
        return Ok(());
    }

    // ── Location picker callbacks (bdloc:…) ──────────────────────────────
    if calendar::is_location_picker_callback(data) {
        let action = match LocationAction::decode(data) {
            Some(a) => a,
            None => {
                bot.answer_callback_query(q.id).await?;
                return Ok(());
            }
        };

        match action {
            LocationAction::SelectCity(city) => {
                let user_id = q.from.id.0;
                state.user_contexts.entry(user_id).or_default().location = Some(city.clone());

                let chat_id = q.message.as_ref().map(|m| m.chat().id).unwrap_or(ChatId(0));
                let msg_id = q.message.as_ref().map(|m| m.id());
                let state_clone = state.clone();
                let bot_clone = bot.clone();
                let user_display_name = get_display_name(&q.from);
                tokio::spawn(async move {
                    let _ = bazi_flow::perform_bazi_analysis(state_clone, bot_clone, chat_id, user_id, user_display_name, msg_id).await;
                });
            }
            LocationAction::Skip => {
                let user_id = q.from.id.0;
                if let Some(mut ctx) = state.user_contexts.get_mut(&user_id) {
                    ctx.location = None;
                } // Default to standard time

                let chat_id = q.message.as_ref().map(|m| m.chat().id).unwrap_or(ChatId(0));
                let msg_id = q.message.as_ref().map(|m| m.id());
                let state_clone = state.clone();
                let bot_clone = bot.clone();
                let user_display_name = get_display_name(&q.from);
                tokio::spawn(async move {
                    let _ = bazi_flow::perform_bazi_analysis(state_clone, bot_clone, chat_id, user_id, user_display_name, msg_id).await;
                });
            }
        }
        bot.answer_callback_query(q.id).await?;
        return Ok(());
    }

    // ── Time picker callbacks (bdtime:…) ──────────────────────────────────
    if calendar::is_time_picker_callback(data) {
        let action = match TimeAction::decode(data) {
            Some(a) => a,
            None => {
                bot.answer_callback_query(q.id).await?;
                return Ok(());
            }
        };

        match action {
            TimeAction::SelectHour(hour) => {
                let user_id = q.from.id.0;
                state.user_contexts.entry(user_id).or_default().hour = Some(hour as u8);
                let markup = calendar::build_minute_picker(hour);
                if let Some(msg) = &q.message {
                    let _ = bot
                        .edit_message_text(msg.chat().id, msg.id(), format!("🕐 Step 5/6 — Selected hour: {:02}:xx\nNow select exact minute:", hour))
                        .reply_markup(markup)
                        .await;
                }
            }
            TimeAction::SelectMinute { hour, minute } => {
                let user_id = q.from.id.0;
                state.user_contexts.entry(user_id).or_default().hour = Some(hour as u8);
                state.user_contexts.entry(user_id).or_default().minute = Some(minute as u8);

                let markup = calendar::build_location_picker();
                if let Some(msg) = &q.message {
                    let _ = bot
                        .edit_message_text(
                            msg.chat().id,
                            msg.id(),
                            format!("📍 Step 6/6 — Time selected: {:02}:{:02}\n\nSelect birth city for True Solar Time (真太阳时):", hour, minute),
                        )
                        .reply_markup(markup)
                        .await;
                }
            }
            TimeAction::BackToHour => {
                let markup = calendar::build_hour_picker();
                let user_id = q.from.id.0;
                let date_str = state.user_contexts.get(&user_id).and_then(|c| c.birthdate.clone()).unwrap_or_else(|| "Selected Date".to_string());

                if let Some(msg) = &q.message {
                    let _ = bot
                        .edit_message_text(msg.chat().id, msg.id(), format!("🕐 Step 5/6 — Select birth hour for {}:", date_str))
                        .reply_markup(markup)
                        .await;
                }
            }
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
        None => {
            bot.answer_callback_query(q.id).await?;
            return Ok(());
        }
    };

    // Answer the callback query immediately to stop the loading spinner on the button
    // BEFORE starting the long LLM generation process.
    let _ = bot.answer_callback_query(q.id.clone()).await;

    match action {
        CalendarAction::SelectDate(date) => {
            let formatted_date = date.format("%Y-%m-%d").to_string();
            process_date_selection(&bot, &q, &state, &formatted_date, "calendar_date", "📝 盲派命理分析：").await?;
        }

        CalendarAction::Today => {
            let today = chrono::Local::now().date_naive();
            let formatted_date = today.format("%Y-%m-%d").to_string();
            process_date_selection(&bot, &q, &state, &formatted_date, "calendar_today", "📝 今日盲派分析：").await?;
        }

        CalendarAction::PrevMonth { year, month } | CalendarAction::NextMonth { year, month } => {
            let markup = calendar::build_calendar(year, month);
            if let Some(msg) = &q.message {
                let _ = bot.edit_message_reply_markup(msg.chat().id, msg.id()).reply_markup(markup).await;
            }
        }
    }

    Ok(())
}

// ─────────────────────────────────────────────
// Shared date selection processing (used by SelectDate)
// ─────────────────────────────────────────────
async fn process_date_selection(bot: &Bot, q: &CallbackQuery, state: &Arc<AppState>, formatted_date: &str, request_type: &str, response_prefix: &str) -> ResponseResult<()> {
    let user = &q.from;
    let user_id = user.id.0;
    info!("User {} selected date: {}", user_id, formatted_date);
    if let Some(msg) = &q.message {
        let chat_id = msg.chat().id;
        let msg_id = msg.id();
        let _ = bot.edit_message_text(chat_id, msg_id, format!("Processing date: {}", formatted_date)).await;

        let ref_content = build_history_msg(&state.user_contexts, user_id);
        let (user_profile_bazi_four_pillars, destiny_reading) = repos::get_user_profile(&state.db_pool, user_id).await;
        let Some(user_bazi_four_pillars_raw) = user_profile_bazi_four_pillars.as_deref() else {
            let _ = bot
                .send_message(chat_id, "⚠️ You haven't set your birthdate yet! Please use /new to set your birthdate first to get accurate readings.")
                .await;
            return Ok(());
        };
        // let user_bazi_four_pillars = utils::get_formatted_bazi_four_pillars(user_bazi_four_pillars_raw);
        let destiny_reading = destiny_reading.unwrap_or_default();

        match llm_bazi::generate_bazi_reading(llm_bazi::BaziReadingRequest {
            http_client: &state.http_client,
            date_value: formatted_date,
            history_msg: &ref_content,
            user_bazi_four_pillars: &user_bazi_four_pillars_raw,
            destiny_reading: &destiny_reading,
            api_key: &state.config.openai_api_key,
            api_base: &state.config.openai_api_base,
            model_name: &state.config.llm_model_name,
        })
        .await
        {
            Ok(result_text) => {
                repos::save_request(&state.db_pool, user_id, request_type, Some(formatted_date), Some(&ref_content), Some(&result_text)).await;
                bot.send_message(chat_id, format!("{}\n{}", response_prefix, result_text)).await?;
            }
            Err(e) => {
                error!("Error: {}", e);
                repos::save_request(&state.db_pool, user_id, request_type, Some(formatted_date), Some(&ref_content), Some(&format!("Error: {}", e))).await;
                bot.send_message(chat_id, format!("Error generating reading: {}", e)).await?;
            }
        }
    }
    Ok(())
}
