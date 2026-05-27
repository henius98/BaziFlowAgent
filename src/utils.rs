//! Shared utility functions used across bot handlers, scheduler, and other modules.
use serde_json::Value;

/// Split a long message into chunks that fit within Telegram's message size limit.
pub fn split_message(text: &str, limit: usize) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();

    for line in text.lines() {
        if current.len() + line.len() > limit {
            result.push(std::mem::take(&mut current));
        }
        current.push_str(line);
        current.push('\n');
    }

    if !current.is_empty() {
        result.push(current);
    }

    if result.is_empty() {
        result.push(text.to_string());
    }

    result
}

/// Extract a string field from a JSON Value
pub fn json_str<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(|s| s.as_str()).unwrap_or("")
}
