use crate::models::UserContext;
use dashmap::DashMap;

/// Build a history message string from stored user conversation contexts.
pub fn build_history_msg(user_contexts: &DashMap<u64, UserContext>, user_id: u64) -> String {
    if let Some(ctx) = user_contexts.get(&user_id)
        && !ctx.messages.is_empty()
    {
        return format!("Here are the previous message:\n{}", ctx.messages.join("\n"));
    }
    String::new()
}

/// Get a display-friendly name for a Telegram user.
pub fn get_display_name(user: &teloxide::types::User) -> String {
    if let Some(username) = &user.username {
        return format!("@{}", username);
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
