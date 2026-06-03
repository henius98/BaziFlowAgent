use crate::repos;
use teloxide::prelude::*;
use tracing::{debug, error};

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

    // Intercept pick activity text
    let mut is_pick_activity = false;
    {
        let mut ctx = state.user_contexts.entry(user_id).or_default();
        if ctx.pick_state.waiting_for_text {
            ctx.pick_state.activity = Some(text.to_string());
            ctx.pick_state.waiting_for_text = false;
            is_pick_activity = true;
        }
    }
    if is_pick_activity {
        let bot_clone = bot.clone();
        let chat_id = msg.chat.id;
        let reply_id = msg.id;

        tokio::spawn(async move {
            let target = Some(super::command_actions::MessageTarget::Reply(reply_id));
            let _ = super::command_actions::process_pick_selection(bot_clone, chat_id, user_id, target).await;
        });
        return Ok(());
    }

    // Performance optimization: cap context at max_context_messages per user
    {
        let mut ctx = state.user_contexts.entry(user_id).or_default();
        ctx.push_message(format!("User: {}", text), state.config.max_context_messages);
        ctx.last_active = chrono::Utc::now();
    }
    debug!("Stored message from {}: {}", user_id, text);

    let user_profile = repos::get_user_profile(&state.db_pool, user_id).await;
    let bazi_four_pillars = user_profile.bazi_four_pillars;
    let llm_model = user_profile.llm_model;
    let Some(_bazi_four_pillars) = bazi_four_pillars.as_deref() else {
        let _ = bot
            .send_message(
                msg.chat.id,
                "⚠️ You haven't set your birthdate yet! Please use /new to set your birthdate first to get accurate readings.",
            )
            .await;
        return Ok(());
    };

    let system_prompt_text = include_str!("../../prompts/FollowUpAssistant.md");

    let system_msg = match async_openai::types::chat::ChatCompletionRequestSystemMessageArgs::default().content(system_prompt_text).build() {
        Ok(m) => m,
        Err(e) => {
            error!("Failed to build system message: {}", e);
            let _ = bot.send_message(msg.chat.id, "Internal error: Failed to build prompt").await;
            return Ok(());
        }
    };

    let mut messages: Vec<async_openai::types::chat::ChatCompletionRequestMessage> = vec![system_msg.into()];

    {
        if let Some(ctx) = state.user_contexts.get(&user_id) {
            for m in &ctx.messages {
                if let Some(stripped) = m.strip_prefix("User: ") {
                    if let Ok(msg) = async_openai::types::chat::ChatCompletionRequestUserMessageArgs::default().content(stripped).build() {
                        messages.push(msg.into());
                    }
                } else if let Some(stripped) = m.strip_prefix("Assistant: ") {
                    if let Ok(msg) = async_openai::types::chat::ChatCompletionRequestAssistantMessageArgs::default().content(stripped).build() {
                        messages.push(msg.into());
                    }
                } else if let Ok(msg) = async_openai::types::chat::ChatCompletionRequestUserMessageArgs::default().content(m.as_str()).build() {
                    messages.push(msg.into());
                }
            }
        }
    }

    let model_name = llm_model.map(|m| m.as_str().to_string()).unwrap_or_else(|| state.config.llm_model_name.clone());
    let mut params = crate::services::llm::LlmRequestParams::new(model_name, messages);
    params.stream = Some(true);
    params.temperature = Some(0.4);
    params.user_id = Some(user_id as i64);
    params.request_type = Some(text.to_string());

    match crate::services::llm::call_llm(&state.db_pool, &state.config.llm_client_config, params).await {
        Ok(crate::models::LlmResponse::Stream(receiver)) => {
            let result_text = super::helpers::stream_to_telegram(&bot, msg.chat.id, "📝 Generating...", receiver).await;

            // Save assistant response to context for future follow-ups
            {
                let mut ctx = state.user_contexts.entry(user_id).or_default();
                ctx.push_message(format!("Assistant: {}", result_text), state.config.max_context_messages);
            }
        }
        Ok(_) => {
            error!("Expected stream response from LLM, got non-stream variant");
            let _ = bot.send_message(msg.chat.id, "Internal error: unexpected response type").await;
        }
        Err(e) => {
            error!("Error generating reading: {}", e);
            let _ = bot.send_message(msg.chat.id, format!("Error processing request: {}", e)).await;
        }
    }

    Ok(())
}
