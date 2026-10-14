use super::Bot;
use teloxide::prelude::*;

// ─────────────────────────────────────────────
// Message handler
// ─────────────────────────────────────────────
pub async fn handle_message(bot: Bot, state: std::sync::Arc<crate::models::AppState>, msg: Message) -> ResponseResult<()> {
  let user_id = msg.from.as_ref().map(|u| u.id.0).unwrap_or(0);
  if user_id == 0 {
    return Ok(());
  }

  let text = match msg.text() {
    Some(t) if !t.starts_with('/') => t,
    _ => return Ok(()),
  };

  // Concurrency & Rate Limiting Check
  let _guard = match crate::models::ProcessingGuard::try_acquire(state.clone(), user_id) {
    Ok(guard) => guard,
    Err(rejection) => {
      let _ = bot.send_message(msg.chat.id, rejection.user_message()).await;
      return Ok(());
    }
  };

  let (effort, content) = crate::models::ReasoningEffortLevel::extract_leading_placeholder(text);
  let content = content.trim();
  if content.is_empty() {
    return Ok(());
  }

  // Intercept pick activity text
  let is_pick_activity = if let Some(mut wizard) = state.pick_wizards.get_mut(&user_id) {
    if wizard.waiting_for_text {
      wizard.activity = Some(content.to_string());
      wizard.waiting_for_text = false;
      wizard.last_active = std::time::Instant::now();
      true
    } else {
      false
    }
  } else {
    false
  };

  let chat_id = msg.chat.id;
  let bot_clone = bot.clone();

  if is_pick_activity {
    let target = Some(super::command_actions::MessageTarget::Reply(msg.id));
    let state_clone = state.clone();
    state.runtime.tasks.spawn(async move {
      let _guard = _guard;
      let _ = super::run(state_clone.clone(), super::command_actions::process_pick_selection(bot_clone, &state_clone, chat_id, user_id, target, effort)).await;
    });
    return Ok(());
  }

  // General chat turn with streaming response
  let state_clone = state.clone();
  let content_string = content.to_string();

  state.runtime.tasks.spawn(async move {
    let _guard = _guard;
    let stream = match crate::services::conversation::submit_chat_turn(&state_clone, user_id, &content_string, true, crate::models::LlmRequestType::FollowUpChat, effort).await {
      Ok(crate::models::LlmResponse::Stream(s)) => s,
      Ok(crate::models::LlmResponse::Full(full_text)) => {
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        let _ = tx.send(tokio_util::bytes::Bytes::from(full_text)).await;
        crate::models::LlmStream::completed(rx)
      }
      Err(crate::models::AppError::ProfileNotFound) => {
        let _ = bot_clone.send_message(chat_id, "⚠️ You haven't set your birthdate yet! Please use /new to set your birthdate first to get accurate readings.").await;
        return;
      }
      Err(e) => {
        tracing::error!("Error generating reading: {}", e);
        let _ = bot_clone.send_message(chat_id, format!("Error processing request: {}", e)).await;
        return;
      }
    };

    super::helpers::stream_to_telegram(&bot_clone, &state_clone, chat_id, "📝 Generating...", stream).await;
  });

  Ok(())
}
