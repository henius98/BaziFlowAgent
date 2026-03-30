use reqwest::Client;
use serde_json::Value;
use tracing::{error, info};

/// Send data to the Dify webhook endpoint.
///
/// # Arguments
/// * `client` - Shared HTTP client
/// * `webhook_url` - The Dify webhook URL
/// * `date_value` - Formatted date string (YYYY-MM-DD)
/// * `history_msg` - Optional previous message context
///
/// # Returns
/// The parsed JSON response from Dify, or an error string.
pub async fn send_to_dify(
    client: &Client,
    webhook_url: &str,
    date_value: &str,
    history_msg: &str,
) -> Result<Value, String> {
    // payload matches the variables you set in Dify's "Start" node
    let payload = serde_json::json!({
        "target_date": date_value,
        "history_msg": history_msg
    });

    let response = client
        .post(webhook_url)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    if !status.is_success() {
        return Err(format!("Bad status {}: {}", status, text));
    }

    serde_json::from_str(&text).map_err(|_| text)
}

/// Extract the result text from a Dify response.
///
/// Handles multiple response formats:
/// - `{ "message": "..." }` - Standard message response
/// - `{ "answer": "..." }` - Answer format
/// - `{ "data": { "outputs": { "text": "..." } } }` - Workflow API response
pub fn extract_dify_result(response: &Value) -> String {
    // Standard Dify Workflow API response structure check
    if let Some(msg) = response.get("message").and_then(|v| v.as_str()) {
        return msg.to_string();
    }

    if let Some(answer) = response.get("answer").and_then(|v| v.as_str()) {
        return answer.to_string();
    }

    if let Some(text) = response
        .get("data")
        .and_then(|d| d.get("outputs"))
        .and_then(|o| o.get("text"))
        .and_then(|t| t.as_str())
    {
        return text.to_string();
    }

    // Fallback for direct webhook response which might return outputs directly
    response.to_string()
}
