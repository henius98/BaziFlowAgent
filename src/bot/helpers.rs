use teloxide::prelude::*;
use teloxide::types::MessageId;

/// Get a display-friendly name for a Telegram user.
pub fn get_username(user: &teloxide::types::User) -> String {
    if let Some(username) = &user.username {
        return username.clone();
    }
    let mut name = user.first_name.clone();
    if let Some(last) = &user.last_name {
        name.push(' ');
        name.push_str(last);
    }
    if name.is_empty() {
        return user.id.to_string();
    }
    name
}

/// Stream LLM output to a Telegram message with progressive edits.
///
/// Sends an initial placeholder, then throttles `editMessageText` calls to ~1.5s
/// intervals to respect Telegram rate limits (~30 edits/min per chat).
/// Returns the full accumulated text on completion.
///
/// If the final text exceeds 4096 chars, the first message is trimmed and overflow
/// is sent as separate messages.
pub async fn stream_to_telegram(bot: &Bot, chat_id: ChatId, msg_id: MessageId, mut receiver: tokio::sync::mpsc::Receiver<String>) -> String {
    let mut accumulated = String::new();
    let edit_interval = std::time::Duration::from_millis(1000);
    // Allow the first chunk to be flushed immediately
    let mut last_edit = std::time::Instant::now().checked_sub(edit_interval).unwrap_or_else(std::time::Instant::now);
    let mut pending = false;

    loop {
        let sleep_dur = edit_interval.saturating_sub(last_edit.elapsed());

        tokio::select! {
            chunk_opt = receiver.recv() => {
                match chunk_opt {
                    Some(chunk) => {
                        accumulated.push_str(&chunk);
                        pending = true;

                        // Throttle: flush immediately if enough time has passed
                        if last_edit.elapsed() >= edit_interval {
                            flush_edit(bot, chat_id, msg_id, &accumulated).await;
                            last_edit = std::time::Instant::now();
                            pending = false;
                        }
                    }
                    None => {
                        // Stream ended — channel closed
                        break;
                    }
                }
            }
            _ = tokio::time::sleep(sleep_dur), if pending => {
                // Time to flush pending content
                flush_edit(bot, chat_id, msg_id, &accumulated).await;
                last_edit = std::time::Instant::now();
                pending = false;
            }
        }
    }

    // Final edit: remove cursor, handle overflow
    if accumulated.is_empty() {
        let _ = bot.edit_message_text(chat_id, msg_id, "⚠️ No content received from LLM.").await;
    } else if accumulated.len() <= 4096 {
        let _ = bot.edit_message_text(chat_id, msg_id, &accumulated).await;
    } else {
        // Edit first message with first ~4000 chars, send remainder as new messages
        let mut split_idx = 4000;
        while !accumulated.is_char_boundary(split_idx) {
            split_idx -= 1;
        }
        let _ = bot.edit_message_text(chat_id, msg_id, &accumulated[..split_idx]).await;
        let parts = crate::utils::split_message(&accumulated[split_idx..], 4000);
        for part in parts {
            let _ = bot.send_message(chat_id, part).await;
        }
    }

    accumulated
}

/// Send a throttled edit with a typing cursor appended.
async fn flush_edit(bot: &Bot, chat_id: ChatId, msg_id: MessageId, text: &str) {
    // Telegram message limit is 4096 chars; show tail if exceeding
    let display = if text.len() <= 4000 {
        format!("{}...", text)
    } else {
        let mut start_idx = text.len().saturating_sub(4000);
        while start_idx < text.len() && !text.is_char_boundary(start_idx) {
            start_idx += 1;
        }
        format!("…{}...", &text[start_idx..])
    };
    let _ = bot.edit_message_text(chat_id, msg_id, &display).await;
}
