use teloxide::prelude::*;

use super::command_actions;
use super::helpers::get_username;
use super::keyboards::{self, BirthdateCalAction, CalendarAction, GenderAction, LocationAction, ModelAction, TimeAction};
use crate::repos;

// ─────────────────────────────────────────────
// Callback handler (calendar + time picker)
// ─────────────────────────────────────────────
pub async fn handle_callback(bot: Bot, q: CallbackQuery) -> ResponseResult<()> {
    let state = crate::models::get_state();
    let data = match q.data.as_deref() {
        Some(d) => d,
        None => return Ok(()),
    };

    // ── Gender picker callbacks (bdgen:…) ──────────────────────────────────
    if keyboards::is_gender_picker_callback(data) {
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
                state.user_contexts.entry(user_id).or_default().profile_state.gender = Some(gender_val);

                let markup = keyboards::build_year_picker(1996);
                if let Some(msg) = &q.message {
                    let _ = bot.edit_message_text(msg.chat().id, msg.id(), "📅 Step 2/6 — Select your birth year:").reply_markup(markup).await;
                }
            }
        }
        bot.answer_callback_query(q.id).await?;
        return Ok(());
    }

    // ── Birthdate calendar callbacks (bdcal:…) ────────────────────────────
    if keyboards::is_birthdate_cal_callback(data) {
        let action = match BirthdateCalAction::decode(data) {
            Some(a) => a,
            None => {
                bot.answer_callback_query(q.id).await?;
                return Ok(());
            }
        };

        match action {
            BirthdateCalAction::ViewYears { start_year } => {
                let markup = keyboards::build_year_picker(start_year);
                if let Some(msg) = &q.message {
                    let _ = bot.edit_message_reply_markup(msg.chat().id, msg.id()).reply_markup(markup).await;
                }
            }
            BirthdateCalAction::SelectYear(year) => {
                let markup = keyboards::build_month_picker(year);
                if let Some(msg) = &q.message {
                    let _ = bot
                        .edit_message_text(msg.chat().id, msg.id(), format!("📅 Step 3/6 — Year: {}\nNow select your birth month:", year))
                        .reply_markup(markup)
                        .await;
                }
            }
            BirthdateCalAction::SelectMonth { year, month } => {
                let markup = keyboards::build_birthdate_calendar(year, month);
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
                state.user_contexts.entry(user_id).or_default().profile_state.birthdate = Some(date_str.clone());

                let markup = keyboards::build_hour_picker(|h| keyboards::TimeAction::SelectHour(h).encode());
                if let Some(msg) = &q.message {
                    let _ = bot
                        .edit_message_text(msg.chat().id, msg.id(), format!("🕐 Step 5/6 — Select birth hour for {}:", date_str))
                        .reply_markup(markup)
                        .await;
                }
            }
            BirthdateCalAction::PrevMonth { year, month } | BirthdateCalAction::NextMonth { year, month } => {
                let markup = keyboards::build_birthdate_calendar(year, month);
                if let Some(msg) = &q.message {
                    let _ = bot.edit_message_reply_markup(msg.chat().id, msg.id()).reply_markup(markup).await;
                }
            }
        }
        bot.answer_callback_query(q.id).await?;
        return Ok(());
    }

    // ── Location picker callbacks (bdloc:…) ──────────────────────────────
    if keyboards::is_location_picker_callback(data) {
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
                state.user_contexts.entry(user_id).or_default().profile_state.location = Some(city.clone());

                let chat_id = q.message.as_ref().map(|m| m.chat().id).unwrap_or(ChatId(0));
                let msg_id = q.message.as_ref().map(|m| m.id());
                let bot_clone = bot.clone();
                let username = get_username(&q.from);
                tokio::spawn(async move {
                    let _ = command_actions::perform_bazi_analysis(bot_clone, chat_id, user_id, username, msg_id).await;
                });
            }
            LocationAction::Skip => {
                let user_id = q.from.id.0;
                if let Some(mut ctx) = state.user_contexts.get_mut(&user_id) {
                    ctx.profile_state.location = None;
                } // Default to standard time

                let chat_id = q.message.as_ref().map(|m| m.chat().id).unwrap_or(ChatId(0));
                let msg_id = q.message.as_ref().map(|m| m.id());
                let bot_clone = bot.clone();
                let username = get_username(&q.from);
                tokio::spawn(async move {
                    let _ = command_actions::perform_bazi_analysis(bot_clone, chat_id, user_id, username, msg_id).await;
                });
            }
        }
        bot.answer_callback_query(q.id).await?;
        return Ok(());
    }

    // ── Time picker callbacks (bdtime:…) ──────────────────────────────────
    if keyboards::is_time_picker_callback(data) {
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
                state.user_contexts.entry(user_id).or_default().profile_state.hour = Some(hour as u8);
                let markup = keyboards::build_minute_picker(
                    hour,
                    |h, m| keyboards::TimeAction::SelectMinute { hour: h, minute: m }.encode(),
                    || keyboards::TimeAction::BackToHour.encode(),
                );
                if let Some(msg) = &q.message {
                    let _ = bot
                        .edit_message_text(msg.chat().id, msg.id(), format!("🕐 Step 5/6 — Selected hour: {:02}:xx\nNow select exact minute:", hour))
                        .reply_markup(markup)
                        .await;
                }
            }
            TimeAction::SelectMinute { hour, minute } => {
                let user_id = q.from.id.0;
                state.user_contexts.entry(user_id).or_default().profile_state.hour = Some(hour as u8);
                state.user_contexts.entry(user_id).or_default().profile_state.minute = Some(minute as u8);

                let markup = keyboards::build_location_picker();
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
                let markup = keyboards::build_hour_picker(|h| keyboards::TimeAction::SelectHour(h).encode());
                let user_id = q.from.id.0;
                let date_str = state
                    .user_contexts
                    .get(&user_id)
                    .and_then(|c| c.profile_state.birthdate.clone())
                    .unwrap_or_else(|| "Selected Date".to_string());

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

    // ── Model picker callbacks (model:…) ──────────────────────────────────
    if keyboards::is_model_picker_callback(data) {
        let action = match ModelAction::decode(data) {
            Some(a) => a,
            None => {
                bot.answer_callback_query(q.id).await?;
                return Ok(());
            }
        };

        match action {
            ModelAction::Select(m) => {
                let user_id = q.from.id.0;
                repos::update_user_llm_model(&state.db_pool, user_id, m).await;

                let model_name = crate::models::common::LlmModel::from_u8(m).map(|model| model.as_str()).unwrap_or("Unknown");
                if let Some(msg) = &q.message {
                    let _ = bot.edit_message_text(msg.chat().id, msg.id(), format!("✅ LLM Model updated to: {}", model_name)).await;
                }
            }
        }
        bot.answer_callback_query(q.id).await?;
        return Ok(());
    }

    // ── Schedule picker callbacks (schedule_time:…) ────────────────────────
    if keyboards::is_schedule_picker_callback(data) {
        use super::keyboards::ScheduleAction;
        let action = match ScheduleAction::decode(data) {
            Some(a) => a,
            None => {
                bot.answer_callback_query(q.id).await?;
                return Ok(());
            }
        };

        match action {
            ScheduleAction::SelectHour(hour) => {
                let markup = keyboards::build_minute_picker(hour, |h, m| ScheduleAction::SelectMinute { hour: h, minute: m }.encode(), || ScheduleAction::BackToHour.encode());
                if let Some(msg) = &q.message {
                    let _ = bot
                        .edit_message_text(msg.chat().id, msg.id(), format!("⏰ Selected hour: {:02}:xx\nNow select exact minute:", hour))
                        .reply_markup(markup)
                        .await;
                }
            }
            ScheduleAction::SelectMinute { hour, minute } => {
                let user_id = q.from.id.0;
                let schedule_val = Some(format!("0 {} {} * * * *", minute, hour));
                let time_str = format!("{:02}:{:02}", hour, minute);

                repos::update_user_schedule(&state.db_pool, user_id, schedule_val.as_deref()).await;

                // Update scheduler
                crate::scheduler::add_or_update_user_schedule(bot.clone(), user_id, schedule_val.as_deref().unwrap()).await;
                if let Some(msg) = &q.message {
                    let _ = bot.edit_message_text(msg.chat().id, msg.id(), format!("✅ Schedule updated to daily at: {}", time_str)).await;
                }
            }
            ScheduleAction::BackToHour => {
                let markup = keyboards::build_schedule_picker();
                if let Some(msg) = &q.message {
                    let _ = bot
                        .edit_message_text(msg.chat().id, msg.id(), "⏰ Select a time to receive your daily Bazi fortune reading:")
                        .reply_markup(markup)
                        .await;
                }
            }
            ScheduleAction::Disable => {
                let user_id = q.from.id.0;
                repos::update_user_schedule(&state.db_pool, user_id, None).await;
                crate::scheduler::remove_user_daily_job(user_id).await;
                if let Some(msg) = &q.message {
                    let _ = bot.edit_message_text(msg.chat().id, msg.id(), "✅ Daily schedule disabled.").await;
                }
            }
        }
        bot.answer_callback_query(q.id).await?;
        return Ok(());
    }

    // ── Pick calendar callbacks (pcal:…) ──────────────────────────────────
    if keyboards::is_pick_calendar_callback(data) {
        use super::keyboards::PickCalendarAction;
        use chrono::Datelike;
        let action = match PickCalendarAction::decode(data) {
            Some(a) => a,
            None => {
                bot.answer_callback_query(q.id).await?;
                return Ok(());
            }
        };

        match action {
            PickCalendarAction::SelectDate(_) | PickCalendarAction::Today | PickCalendarAction::Tomorrow => {
                let date = match action {
                    PickCalendarAction::SelectDate(d) => d,
                    PickCalendarAction::Today => chrono::Local::now().date_naive(),
                    PickCalendarAction::Tomorrow => chrono::Local::now().date_naive() + chrono::Duration::days(1),
                    _ => unreachable!(),
                };

                let user_id = q.from.id.0;
                let date_str = date.format("%Y-%m-%d").to_string();

                let start_date = {
                    let mut ctx = state.user_contexts.entry(user_id).or_default();
                    if ctx.pick_state.start_date.is_none() {
                        ctx.pick_state.start_date = Some(date_str.clone());
                        None
                    } else {
                        ctx.pick_state.start_date.clone()
                    }
                };

                if let Some(start) = start_date {
                    // This is the end date selection
                    let s_date = match chrono::NaiveDate::parse_from_str(&start, "%Y-%m-%d") {
                        Ok(d) => d,
                        Err(e) => {
                            tracing::error!("Failed to parse start date {}: {}", start, e);
                            if let Some(msg) = &q.message {
                                let _ = bot.edit_message_text(msg.chat().id, msg.id(), "⚠️ Invalid start date encountered.").await;
                            }
                            return Ok(());
                        }
                    };
                    let diff = (date - s_date).num_days();

                    if diff < 0 {
                        // End date before start date
                        if let Some(msg) = &q.message {
                            let _ = bot
                                .edit_message_text(msg.chat().id, msg.id(), "⚠️ End Date must be after Start Date! Please select a valid End Date:")
                                .reply_markup(keyboards::build_pick_calendar(date.year(), date.month()))
                                .await;
                        }
                    } else if diff > 14 {
                        // Max 14 days
                        if let Some(msg) = &q.message {
                            let _ = bot
                                .edit_message_text(
                                    msg.chat().id,
                                    msg.id(),
                                    format!("⚠️ Date range too large ({} days). Max is 14 days. Please select a closer End Date:", diff),
                                )
                                .reply_markup(keyboards::build_pick_calendar(date.year(), date.month()))
                                .await;
                        }
                    } else {
                        // Valid end date
                        state.user_contexts.entry(user_id).or_default().pick_state.end_date = Some(date_str.clone());
                        let markup = keyboards::build_activity_picker();
                        if let Some(msg) = &q.message {
                            let _ = bot
                                .edit_message_text(msg.chat().id, msg.id(), format!("🎯 Step 3/3 — Date Range: {} to {}\n\nSelect your target activity:", start, date_str))
                                .reply_markup(markup)
                                .await;
                        }
                    }
                } else {
                    // We just set the start date, ask for end date
                    let markup = keyboards::build_pick_calendar(date.year(), date.month());
                    if let Some(msg) = &q.message {
                        let _ = bot
                            .edit_message_text(msg.chat().id, msg.id(), format!("🎯 Step 2/3 — Start Date: {}\n\nPlease select the End Date:", date_str))
                            .reply_markup(markup)
                            .await;
                    }
                }
            }
            PickCalendarAction::PrevMonth { year, month } | PickCalendarAction::NextMonth { year, month } => {
                let markup = keyboards::build_pick_calendar(year, month);
                if let Some(msg) = &q.message {
                    let _ = bot.edit_message_reply_markup(msg.chat().id, msg.id()).reply_markup(markup).await;
                }
            }
        }
        bot.answer_callback_query(q.id).await?;
        return Ok(());
    }

    // ── Pick activity callbacks (pact:…) ──────────────────────────────────
    if keyboards::is_pick_activity_callback(data) {
        use super::keyboards::PickActivityAction;
        let action = match PickActivityAction::decode(data) {
            Some(a) => a,
            None => {
                bot.answer_callback_query(q.id).await?;
                return Ok(());
            }
        };

        match action {
            PickActivityAction::Select(activity) => {
                let user_id = q.from.id.0;
                state.user_contexts.entry(user_id).or_default().pick_state.activity = Some(activity.clone());

                let chat_id = q.message.as_ref().map(|m| m.chat().id).unwrap_or(ChatId(0));
                let msg_id = q.message.as_ref().map(|m| m.id());
                let bot_clone = bot.clone();

                // Spawn a task to process the selection
                tokio::spawn(async move {
                    let target = msg_id.map(command_actions::MessageTarget::Edit);
                    let _ = command_actions::process_pick_selection(bot_clone, chat_id, user_id, target).await;
                });
            }
            PickActivityAction::Other => {
                let user_id = q.from.id.0;
                state.user_contexts.entry(user_id).or_default().pick_state.waiting_for_text = true;

                if let Some(msg) = &q.message {
                    let _ = bot
                        .edit_message_text(
                            msg.chat().id,
                            msg.id(),
                            "📝 Please type your target activity in the chat (e.g. 'Meeting with client', 'Surgery', 'Buying house'):",
                        )
                        .await;
                }
            }
        }
        bot.answer_callback_query(q.id).await?;
        return Ok(());
    }

    // ── Bazi analysis calendar callbacks (cal:…) ─────────────────────────
    if !keyboards::is_calendar_callback(data) {
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
            command_actions::process_date_selection(&bot, &q, &formatted_date, "calendar_date", "📝 盲派命理分析：").await?;
        }

        CalendarAction::Today => {
            let today = chrono::Local::now().date_naive();
            let formatted_date = today.format("%Y-%m-%d").to_string();
            command_actions::process_date_selection(&bot, &q, &formatted_date, "calendar_today", "📝 今日盲派分析：").await?;
        }

        CalendarAction::Tomorrow => {
            let tomorrow = chrono::Local::now().date_naive() + chrono::Duration::days(1);
            let formatted_date = tomorrow.format("%Y-%m-%d").to_string();
            command_actions::process_date_selection(&bot, &q, &formatted_date, "calendar_tomorrow", "📝 明日盲派分析：").await?;
        }

        CalendarAction::PrevMonth { year, month } | CalendarAction::NextMonth { year, month } => {
            let markup = keyboards::build_calendar(year, month);
            if let Some(msg) = &q.message {
                let _ = bot.edit_message_reply_markup(msg.chat().id, msg.id()).reply_markup(markup).await;
            }
        }
    }

    Ok(())
}
