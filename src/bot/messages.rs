use std::sync::Arc;
use teloxide::prelude::*;
use tracing::{error, info};

use super::calendar;
use super::helpers::build_history_msg;
use crate::models::AppState;
use crate::repos;
use crate::services::llm_bazi;
use crate::utils;

// ─────────────────────────────────────────────
// Message handler
// ─────────────────────────────────────────────

pub async fn handle_message(bot: Bot, msg: Message, state: Arc<AppState>) -> ResponseResult<()> {
    let user_id = msg.from.as_ref().map(|u| u.id.0).unwrap_or(0);
    if user_id == 0 {
        return Ok(());
    }

    let text = match msg.text() {
        Some(t) if !t.starts_with('/') => t,
        _ => return Ok(()),
    };

    // If user has a pending birthdate, check if they are providing the time via text
    if state.user_contexts.get(&user_id).and_then(|c| c.birthdate.clone()).is_some() {
        let parts: Vec<&str> = text.split(':').collect();
        if parts.len() == 2
            && let (Ok(hour), Ok(minute)) = (parts[0].trim().parse::<u8>(), parts[1].trim().parse::<u8>())
            && hour < 24
            && minute < 60
        {
            state.user_contexts.entry(user_id).or_default().hour = Some(hour);
            state.user_contexts.entry(user_id).or_default().minute = Some(minute);

            let markup = calendar::build_location_picker();
            let _ = bot
                .send_message(
                    msg.chat.id,
                    format!("📍 Step 6/6 — Time selected: {:02}:{:02}\n\nSelect birth city for True Solar Time (真太阳时):", hour, minute),
                )
                .reply_markup(markup)
                .await;
            return Ok(());
        }
    }

    // Performance optimization: cap context at max_context_messages per user
    {
        let mut ctx = state.user_contexts.entry(user_id).or_default();
        if ctx.messages.len() >= state.config.max_context_messages {
            ctx.messages.remove(0); // Keep max messages in context
        }
        ctx.messages.push(format!("User: {}", text));
        ctx.last_active = chrono::Utc::now();
    }

    info!("Stored message from {}: {}", user_id, text);

    let today = chrono::Local::now().date_naive().format("%Y-%m-%d").to_string();
    let ref_content = build_history_msg(&state.user_contexts, user_id);
    let (user_bazi_four_pillars_raw, destiny_reading) = repos::get_user_profile(&state.db_pool, user_id).await;
    let Some(user_bazi_four_pillars_raw) = user_bazi_four_pillars_raw.as_deref() else {
        let _ = bot
            .send_message(
                msg.chat.id,
                "⚠️ You haven't set your birthdate yet! Please use /new to set your birthdate first to get accurate readings.",
            )
            .await;
        return Ok(());
    };
    // let user_bazi_four_pillars = utils::get_formatted_bazi_four_pillars(user_bazi_four_pillars_raw);
    let destiny_reading = destiny_reading.unwrap_or_default();

    let _ = bot.send_chat_action(msg.chat.id, teloxide::types::ChatAction::Typing).await;

    match llm_bazi::generate_bazi_reading(llm_bazi::BaziReadingRequest {
        http_client: &state.http_client,
        date_value: &today,
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
            repos::save_request(&state.db_pool, user_id, "message", Some(&today), Some(text), Some(&result_text)).await;
            bot.send_message(msg.chat.id, format!("📝 回复：\n{}", result_text)).await?;
        }
        Err(e) => {
            error!("Error generating reading: {}", e);
            repos::save_request(&state.db_pool, user_id, "message", Some(&today), Some(text), Some(&format!("Error: {}", e))).await;
            bot.send_message(msg.chat.id, format!("Error processing request: {}", e)).await?;
        }
    }

    Ok(())
}
