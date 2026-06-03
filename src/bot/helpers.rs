use teloxide::prelude::*;

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
pub async fn stream_to_telegram(bot: &Bot, chat_id: ChatId, initial_text: &str, mut receiver: tokio::sync::mpsc::Receiver<String>) -> String {
    let mut accumulated = String::new();
    let edit_interval = std::time::Duration::from_millis(1000);
    // Allow the first chunk to be flushed immediately
    let mut last_edit = std::time::Instant::now().checked_sub(edit_interval).unwrap_or_else(std::time::Instant::now);
    let mut pending = false;

    // Generate a unique draft ID for this streaming session
    let draft_id = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_micros() as i64;
    let client = reqwest::Client::new();
    let token = bot.token();

    // Show the custom initial placeholder immediately as a draft
    flush_draft(&client, token, chat_id, draft_id, initial_text, true).await;

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
                            flush_draft(&client, token, chat_id, draft_id, &accumulated, false).await;
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
                flush_draft(&client, token, chat_id, draft_id, &accumulated, false).await;
                last_edit = std::time::Instant::now();
                pending = false;
            }
        }
    }

    // Finalize: send actual message to persist the ephemeral draft
    if accumulated.is_empty() {
        let _ = bot.send_message(chat_id, "⚠️ No content received from LLM.").await;
    } else if accumulated.len() <= 4096 {
        let _ = bot.send_message(chat_id, &accumulated).await;
    } else {
        // Send in multiple messages if >4096
        let parts = crate::utils::split_message(&accumulated, 4000);
        for part in parts {
            let _ = bot.send_message(chat_id, part).await;
        }
    }

    accumulated
}

/// Send a throttled draft edit using the sendMessageDraft API.
async fn flush_draft(client: &reqwest::Client, token: &str, chat_id: ChatId, draft_id: i64, text: &str, is_initial: bool) {
    let display = if text.is_empty() && !is_initial {
        "Thinking...".to_string()
    } else if text.len() <= 4000 {
        format!("{}...", text)
    } else {
        let mut start_idx = text.len().saturating_sub(4000);
        while start_idx < text.len() && !text.is_char_boundary(start_idx) {
            start_idx += 1;
        }
        format!("…{}...", &text[start_idx..])
    };

    let url = format!("https://api.telegram.org/bot{}/sendMessageDraft", token);

    // We send a JSON payload with the required parameters
    let _ = client
        .post(&url)
        .json(&serde_json::json!({
            "chat_id": chat_id.0,
            "draft_id": draft_id,
            "text": display
        }))
        .send()
        .await;
}
