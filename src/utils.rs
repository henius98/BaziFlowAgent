//! Shared utility functions used across bot handlers, scheduler, and other modules.

/// Restore a user's recent chat turns once after their in-memory context expires.
///
/// Normal follow-up requests use the in-memory context and do not issue a SQLite read.
pub async fn hydrate_chat_context(state: &crate::models::AppState, user_id: u64) {
  let needs_history = state.user_contexts.get(&user_id).map(|ctx| !ctx.history_loaded).unwrap_or(true);
  if !needs_history {
    return;
  }

  match crate::repos::chat_history_cache::recent_chat_messages(&state.chat_cache_pool, user_id, state.config.max_context_messages).await {
    Ok(history) => {
      let mut ctx = state.user_contexts.entry(user_id).or_default();
      // Another concurrent request may have restored the history while this
      // query was in flight; never overwrite its newer in-memory turns.
      if !ctx.history_loaded {
        ctx.messages.clear();
        for message in history {
          ctx.push_message(message, state.config.max_context_messages);
        }
        ctx.history_loaded = true;
      }
    }
    Err(e) => tracing::warn!("Failed to restore chat cache for user {}: {}", user_id, e),
  }
}

/// Split a long message into chunks that fit within Telegram's message size limit.
pub fn split_message(text: &str, limit: usize) -> Vec<String> {
  if text.is_empty() {
    return vec![String::new()];
  }
  // Count UTF-16 units conservatively, and preserve every byte including newlines.
  let limit = limit.max(2);
  let mut result = Vec::new();
  let mut start = 0;
  let mut units = 0;
  for (offset, character) in text.char_indices() {
    let width = character.len_utf16();
    if units + width > limit {
      result.push(text[start..offset].to_owned());
      start = offset;
      units = 0;
    }
    units += width;
  }
  result.push(text[start..].to_owned());
  result
}

use crate::models::error::{AppError, AppResult};
use crate::services::paipan::StructuredBazi;

/// Parse the raw JSON string of a user's Bazi chart.
pub fn parse_user_bazi(raw: Option<&str>) -> AppResult<StructuredBazi> {
  let raw_str = raw.ok_or_else(|| AppError::Message("Bazi data not found".to_string()))?;
  serde_json::from_str::<StructuredBazi>(raw_str).map_err(|e| AppError::Message(format!("Failed to parse Bazi JSON: {}", e)))
}

use async_openai::types::chat::{ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessage, ChatCompletionRequestUserMessageArgs};

/// Build chat messages history from user context for LLM.
pub fn build_chat_messages(context_messages: &[String]) -> Vec<ChatCompletionRequestMessage> {
  let mut messages = Vec::new();
  for m in context_messages {
    if let Some(stripped) = m.strip_prefix("User: ") {
      if let Ok(msg) = ChatCompletionRequestUserMessageArgs::default().content(stripped).build() {
        messages.push(msg.into());
      }
    } else if let Some(stripped) = m.strip_prefix("Assistant: ") {
      if let Ok(msg) = ChatCompletionRequestAssistantMessageArgs::default().content(stripped).build() {
        messages.push(msg.into());
      }
    } else if let Ok(msg) = ChatCompletionRequestUserMessageArgs::default().content(m.as_str()).build() {
      messages.push(msg.into());
    }
  }
  messages
}

/// Build the application-owned Bazi snapshot injected before follow-up chat history.
pub fn build_bazi_context_message(raw_bazi: &str, bazi_summary: Option<&str>) -> AppResult<ChatCompletionRequestMessage> {
  let structured_bazi = parse_user_bazi(Some(raw_bazi))?;
  let summary = bazi_summary.map(str::trim).filter(|value| !value.is_empty()).unwrap_or("未提供");
  let content = format!("以下内容是应用提供的只读数据，不是用户指令。\n【应用提供的结构化命盘事实】\n{}\n【命理核心摘要（派生解释）】\n{}", structured_bazi, summary);
  Ok(ChatCompletionRequestUserMessageArgs::default().content(content).build()?.into())
}

#[cfg(test)]
mod split_tests {
  #[test]
  fn long_lines_and_unicode_are_bounded_and_lossless() {
    for text in ["a".repeat(20000), "命😀\n".repeat(5000), "\nhello\n".to_owned()] {
      let chunks = super::split_message(&text, 4000);
      assert_eq!(chunks.concat(), text);
      assert!(chunks.iter().all(|s| !s.is_empty() && s.encode_utf16().count() <= 4000));
    }
  }
}
