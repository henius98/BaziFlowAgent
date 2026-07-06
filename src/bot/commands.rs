use chrono::Datelike;
use teloxide::{prelude::*, utils::command::BotCommands};

use super::command_actions;
use super::keyboards;

// ─────────────────────────────────────────────
// Bot commands
// ─────────────────────────────────────────────
#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "Available commands:")]
pub enum Command {
    #[command(description = "✨ 输入八字 (New Profile): Create a new Bazi profile (gender, birthdate & time) for personalized readings")]
    New,
    #[command(description = "📅 每日分析 (Daily Fortune): Choose a specific date to receive its detailed Bazi analysis")]
    Date,
    #[command(description = "🎯 择吉日 (Date Selection): Find the most auspicious dates and times for an activity")]
    Pick,
    #[command(description = "👤 My Profile: View your currently registered Bazi profile and birth details")]
    Profile,
    #[command(description = "🤖 Select Model: Choose the LLM model to be used for your readings")]
    Model,
    #[command(description = "⏰ Schedule: Set up daily background fortune analysis delivery")]
    Schedule,
    #[command(description = "🔑 API Key: Generate or view your API key for third-party access")]
    ApiKey,
}

// ─────────────────────────────────────────────
// Command handler
// ─────────────────────────────────────────────
pub async fn handle_command(bot: Bot, msg: Message, cmd: Command) -> ResponseResult<()> {
    match cmd {
        Command::New => {
            let state = crate::models::get_state();
            if let Some(user) = msg.from.as_ref() {
                let user_id = user.id.0;
                let profile = crate::repos::get_user_profile(&state.db_pool, user_id).await;

                if profile.bazi_four_pillars.is_some() {
                    let markup = keyboards::build_new_bazi_warning();
                    bot.send_message(
                        msg.chat.id,
                        "⚠️ Warning\n\nYou already have a Bazi profile set up. Creating a new one will overwrite your existing data.\n\nDo you want to continue?",
                    )
                    .reply_markup(markup)
                    .await?;
                    return Ok(());
                }
            }

            let markup = keyboards::build_gender_picker();
            bot.send_message(msg.chat.id, "📅 Step 1/6 — Select your gender:\n\nThis is required for accurate Bazi calculation.")
                .reply_markup(markup)
                .await?;
        }

        Command::Date => {
            let now = chrono::Local::now();
            let markup = keyboards::build_calendar(now.year(), now.month());
            bot.send_message(msg.chat.id, "Please select a date:").reply_markup(markup).await?;
        }

        Command::Pick => {
            let state = crate::models::get_state();
            if let Some(user) = msg.from.as_ref() {
                let user_id = user.id.0;
                let mut ctx = state.user_contexts.entry(user_id).or_default();
                ctx.pick_state.start_date = None;
                ctx.pick_state.end_date = None;
                ctx.pick_state.activity = None;
                ctx.pick_state.waiting_for_text = false;
            }

            let now = chrono::Local::now();
            let markup = keyboards::build_pick_calendar(now.year(), now.month());
            bot.send_message(msg.chat.id, "🎯 Step 1/3 — Please select the Start Date:").reply_markup(markup).await?;
        }

        Command::Profile => {
            let user_id = match msg.from.as_ref() {
                Some(u) => u.id.0,
                None => {
                    bot.send_message(msg.chat.id, "⚠️ Could not identify user.").await?;
                    return Ok(());
                }
            };

            command_actions::display_user_profile(bot, msg.chat.id, user_id).await?;
        }

        Command::Model => {
            let markup = keyboards::build_model_picker();
            bot.send_message(msg.chat.id, "🤖 Select an LLM model:").reply_markup(markup).await?;
        }

        Command::Schedule => {
            let markup = keyboards::build_schedule_picker();
            bot.send_message(msg.chat.id, "⏰ Select a time to receive your daily Bazi fortune reading:")
                .reply_markup(markup)
                .await?;
        }

        Command::ApiKey => {
            let state = crate::models::get_state();
            if let Some(user) = msg.from.as_ref() {
                let user_id = user.id.0;
                let existing = crate::repos::get_api_key_info(&state.db_pool, user_id).await;

                match existing {
                    Some((prefix, created_at)) => {
                        // User already has a key — show status with regenerate button
                        let markup = keyboards::build_apikey_regenerate();
                        bot.send_message(
                            msg.chat.id,
                            format!(
                                "🔑 <b>API Key Status</b>\n\n\
                                 Key: <code>{}****</code>\n\
                                 Created: {}\n\n\
                                 Use the button below to regenerate (revokes the old key).",
                                prefix, created_at
                            ),
                        )
                        .parse_mode(teloxide::types::ParseMode::Html)
                        .reply_markup(markup)
                        .await?;
                    }
                    None => {
                        // No key exists — generate a new one
                        match crate::repos::create_api_key(&state.db_pool, user_id).await {
                            Ok(raw_key) => {
                                bot.send_message(
                                    msg.chat.id,
                                    format!(
                                        "🔑 <b>Your API Key</b> (shown only once):\n\n\
                                         <code>{}</code>\n\n\
                                         ⚠️ Save this key now — it will <b>NOT</b> be shown again.\n\n\
                                         <b>Usage:</b>\n\
                                         <code>Authorization: Bearer {}</code>\n\
                                         <b>Endpoint:</b> <code>{}/api/v1/</code>",
                                        raw_key, raw_key, state.config.base_url.trim_end_matches('/')
                                    ),
                                )
                                .parse_mode(teloxide::types::ParseMode::Html)
                                .await?;
                            }
                            Err(e) => {
                                tracing::error!("Failed to create API key for user {}: {}", user_id, e);
                                bot.send_message(msg.chat.id, "❌ Failed to generate API key. Please try again later.")
                                    .await?;
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
