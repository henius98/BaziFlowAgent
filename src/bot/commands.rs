use chrono::Datelike;
use teloxide::{prelude::*, utils::command::BotCommands};

use super::bazi_flow;
use super::calendar;
use crate::repos;

// ─────────────────────────────────────────────
// Bot commands
// ─────────────────────────────────────────────
#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "Available commands:")]
pub enum Command {
    #[command(description = "输入八字 (New Profile): Create a new Bazi profile (gender, birthdate & time) for personalized readings")]
    New,
    #[command(description = "每日分析 (Daily Bazi): Choose a specific date to receive its detailed Bazi analysis")]
    Date,
    #[command(description = "择吉日 (Date Selection): Find the most auspicious dates and times for an activity")]
    Pick,
    #[command(description = "My Profile: View your currently registered Bazi profile and birth details")]
    Profile,
}

// ─────────────────────────────────────────────
// Command handler
// ─────────────────────────────────────────────
pub async fn handle_command(bot: Bot, msg: Message, cmd: Command) -> ResponseResult<()> {
    let state = crate::models::get_state();
    if let Some(user) = msg.from.as_ref() {
        let user_id = user.id.0;

        repos::save_request(&state.db_pool, user_id, "command", None, msg.text(), None).await;
    }

    match cmd {
        Command::New => {
            let markup = calendar::build_gender_picker();
            bot.send_message(msg.chat.id, "📅 Step 1/6 — Select your gender:\n\nThis is required for accurate Bazi calculation.")
                .reply_markup(markup)
                .await?;
        }

        Command::Date => {
            let now = chrono::Local::now();
            let markup = calendar::build_calendar(now.year(), now.month());
            bot.send_message(msg.chat.id, "Please select a date:").reply_markup(markup).await?;
        }

        Command::Pick => {
            bot.send_message(msg.chat.id, "Pick command is not implemented yet.").await?;
        }

        Command::Profile => {
            let user_id = match msg.from.as_ref() {
                Some(u) => u.id.0,
                None => {
                    bot.send_message(msg.chat.id, "⚠️ Could not identify user.").await?;
                    return Ok(());
                }
            };

            bazi_flow::display_user_profile(bot, msg.chat.id, user_id).await?;
        }
    }
    Ok(())
}
