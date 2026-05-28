use super::helpers::build_history_msg;
use crate::repos;
use teloxide::prelude::*;
use tracing::{error, info};

// ─────────────────────────────────────────────
// Message handler
// ─────────────────────────────────────────────
pub async fn handle_message(bot: Bot, msg: Message) -> ResponseResult<()> {
    let state = crate::models::get_state();
    let user_id = msg.from.as_ref().map(|u| u.id.0).unwrap_or(0);
    if user_id == 0 {
        return Ok(());
    }

    let text = match msg.text() {
        Some(t) if !t.starts_with('/') => t,
        _ => return Ok(()),
    };

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
    let (bazi_four_pillars, bazi_analysis) = repos::get_user_profile(&state.db_pool, user_id).await;
    let Some(bazi_four_pillars) = bazi_four_pillars.as_deref() else {
        let _ = bot
            .send_message(
                msg.chat.id,
                "⚠️ You haven't set your birthdate yet! Please use /new to set your birthdate first to get accurate readings.",
            )
            .await;
        return Ok(());
    };

    let _ = bot.send_chat_action(msg.chat.id, teloxide::types::ChatAction::Typing).await;

    match crate::services::almanac::analysis_date_fortune(crate::services::almanac::DateFortuneRequest {
        target_date: &today,
        bazi_four_pillars,
        bazi_analysis: bazi_analysis.as_deref().unwrap_or_default(),
        history_context: Some(&ref_content),
    })
    .await
    {
        Ok(result_text) => {
            repos::save_request(&state.db_pool, user_id, "message", Some(today.as_str()), Some(text), Some(result_text.as_str())).await;
            bot.send_message(msg.chat.id, format!("📝 回复：\n{}", result_text)).await?;
        }
        Err(e) => {
            error!("Error generating reading: {}", e);
            repos::save_request(&state.db_pool, user_id, "message", Some(today.as_str()), Some(text), Some(format!("Error: {}", e).as_str())).await;
            bot.send_message(msg.chat.id, format!("Error processing request: {}", e)).await?;
        }
    }

    Ok(())
}
