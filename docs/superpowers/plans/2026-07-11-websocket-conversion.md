# WebSocket Conversion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Convert the `/api/v1/date-fortune` endpoint from SSE to WebSockets for bi-directional streaming and cancellation.

**Architecture:** Use `axum::extract::ws::WebSocketUpgrade` to upgrade the HTTP connection to a WebSocket. Define `ClientMessage` and `ServerMessage` JSON structures. Use `tokio::select!` inside the WebSocket stream to concurrently listen for client commands (like "stop") and receive chunks from the LLM receiver channel.

**Tech Stack:** Rust, Axum, Tokio, Serde, async-openai.

## Global Constraints

- Never remove my comment on code, only edit or update the comment.
- Always Keep Single Source of Truth.
- Never commit my code (unless requested in the step).

---

### Task 1: Ensure Axum `ws` feature and define WebSocket messages

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/api/models.rs`

**Interfaces:**
- Consumes: None
- Produces: `ClientMessage`, `ServerMessage` structs for JSON serialization.

- [ ] **Step 1: Check and add `ws` feature to Axum**

Modify `Cargo.toml`. Since we use axum 0.8, ensure `ws` is in the features.
```toml
# Check Cargo.toml around line 63. If it looks like this:
# axum = { version = "0.8", features = ["macros"] }
# Change it to:
axum = { version = "0.8", features = ["macros", "ws"] }
```

- [ ] **Step 2: Define JSON message structures**

Modify `src/api/models.rs` to add `ClientMessage` and `ServerMessage`.
```rust
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ClientMessage {
    Generate { date: String },
    Stop,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Almanac { data: Value },
    Chunk { data: String },
    Done,
    Error { message: String },
}
```

- [ ] **Step 3: Run `cargo check` to verify**

Run: `cargo check`
Expected: Passes without errors.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml src/api/models.rs
git commit -m "feat(api): add WebSocket message models and axum ws feature"
```

---

### Task 2: Implement WebSocket upgrade route for Date Fortune

**Files:**
- Modify: `src/api/handlers.rs`

**Interfaces:**
- Consumes: `ClientMessage`, `ServerMessage` from `src/api/models.rs`.
- Produces: A new WebSocket handler for `/api/v1/date-fortune`.

- [ ] **Step 1: Update endpoint signature**

Modify `src/api/handlers.rs`, changing `date_fortune` to handle WebSocket upgrades.
```rust
use axum::extract::ws::{WebSocket, WebSocketUpgrade, Message};
use crate::api::models::{ClientMessage, ServerMessage};
use futures_util::{SinkExt, StreamExt};

pub async fn date_fortune(
    auth: AuthUser,
    ws: WebSocketUpgrade,
) -> axum::response::Response {
    ws.on_upgrade(move |socket| handle_socket(socket, auth.user_id))
}
```

- [ ] **Step 2: Implement `handle_socket` function**

Add the `handle_socket` function to `src/api/handlers.rs`.
```rust
async fn handle_socket(mut socket: WebSocket, user_id: i64) {
    let state = crate::models::get_state();

    // 1. Wait for Generate message
    let req_date = match socket.next().await {
        Some(Ok(Message::Text(text))) => {
            match serde_json::from_str::<ClientMessage>(&text) {
                Ok(ClientMessage::Generate { date }) => date,
                _ => {
                    let _ = socket.send(Message::Text(serde_json::to_string(&ServerMessage::Error { message: "Expected Generate action".into() }).unwrap().into())).await;
                    return;
                }
            }
        }
        _ => return,
    };

    let user_profile = crate::repos::get_user_profile(&state.db_pool, user_id).await;
    let bazi_four_pillars = user_profile.bazi_four_pillars.as_deref().and_then(|raw| serde_json::from_str::<crate::services::paipan::StructuredBazi>(raw).ok()).map(|b| b.to_string());
    
    let Some(bazi_four_pillars) = bazi_four_pillars.as_deref() else {
        let _ = socket.send(Message::Text(serde_json::to_string(&ServerMessage::Error { message: "No Bazi profile found".into() }).unwrap().into())).await;
        return;
    };

    let almanac_data = match crate::services::almanac::fetch_and_format_almanac(&state.http_client, &req_date).await {
        Ok(data) => data,
        Err(e) => {
            let _ = socket.send(Message::Text(serde_json::to_string(&ServerMessage::Error { message: e.to_string() }).unwrap().into())).await;
            return;
        }
    };

    // Send almanac data
    let almanac_val = serde_json::to_value(&almanac_data).unwrap_or_default();
    let _ = socket.send(Message::Text(serde_json::to_string(&ServerMessage::Almanac { data: almanac_val }).unwrap().into())).await;

    let bazi_summary = user_profile.bazi_summary.as_deref().unwrap_or_else(|| user_profile.bazi_analysis.as_deref().unwrap_or_default());

    let llm_result = crate::services::almanac::analysis_date_fortune(crate::services::almanac::DateFortuneRequest {
        target_date: &req_date,
        almanac_data: &almanac_data,
        bazi_four_pillars,
        bazi_summary,
        stream: true,
        llm_model: user_profile.llm_model,
        user_id: Some(user_id as i64),
        request_type: Some("api_date_fortune".to_string()),
    }).await;

    match llm_result {
        Ok(crate::models::LlmResponse::Stream(mut receiver)) => {
            loop {
                tokio::select! {
                    msg = socket.next() => {
                        match msg {
                            Some(Ok(Message::Text(text))) => {
                                if let Ok(ClientMessage::Stop) = serde_json::from_str::<ClientMessage>(&text) {
                                    // Drop receiver by breaking the loop
                                    break;
                                }
                            }
                            Some(Err(_)) | None => {
                                // Connection closed
                                break;
                            }
                            _ => {}
                        }
                    }
                    chunk = receiver.recv() => {
                        match chunk {
                            Some(c) => {
                                let _ = socket.send(Message::Text(serde_json::to_string(&ServerMessage::Chunk { data: c }).unwrap().into())).await;
                            }
                            None => {
                                let _ = socket.send(Message::Text(serde_json::to_string(&ServerMessage::Done).unwrap().into())).await;
                                break;
                            }
                        }
                    }
                }
            }
        }
        _ => {
            let _ = socket.send(Message::Text(serde_json::to_string(&ServerMessage::Error { message: "Expected stream".into() }).unwrap().into())).await;
        }
    }
}
```

- [ ] **Step 3: Run `cargo check` to verify**

Run: `cargo check`
Expected: Passes without errors.

- [ ] **Step 4: Commit**

```bash
git add src/api/handlers.rs
git commit -m "feat(api): implement WebSocket upgrade for date_fortune endpoint"
```
