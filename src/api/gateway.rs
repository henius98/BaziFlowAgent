//! Version 1 persistent integration protocol. API keys act only as their owner.
use super::auth::AuthUser;
use crate::{
  models::{AppState, events::Connection},
  services::integration,
};
use axum::{
  extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade},
  http::{HeaderMap, StatusCode},
  response::{IntoResponse, Response},
};
use serde::Deserialize;
use std::{
  sync::{Arc, atomic::Ordering},
  time::Duration,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
  pub version: u8,
  pub id: String,
  pub action: Action,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "name", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
  GetState {},
  SetModel { model: u8 },
  Subscribe { event: String },
  Unsubscribe {},
}

pub fn decode(text: &str) -> Result<Request, &'static str> {
  let request: Request = serde_json::from_str(text).map_err(|_| "invalid_request")?;
  if request.version != 1 {
    return Err("unsupported_version");
  }
  if request.id.is_empty() || request.id.len() > 64 || !request.id.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b)) {
    return Err("invalid_id");
  }
  Ok(request)
}

pub fn origin_allowed(headers: &HeaderMap, configured: &str) -> bool {
  let Some(origin) = headers.get("origin") else {
    return true;
  };
  let Ok(origin) = origin.to_str() else {
    return false;
  };
  configured != "*" && configured.split(',').any(|allowed| allowed.trim().trim_end_matches('/') == origin)
}

pub async fn upgrade(auth: AuthUser, headers: HeaderMap, ws: WebSocketUpgrade) -> Response {
  let state = crate::models::get_state();
  if !origin_allowed(&headers, &state.config.cors_allowed_origin) {
    return StatusCode::FORBIDDEN.into_response();
  }
  let Ok(permit) = state.runtime.connections.clone().try_acquire_owned() else {
    return StatusCode::TOO_MANY_REQUESTS.into_response();
  };
  let Some(connection) = state.events.connect(auth.user_id, state.config.runtime.queue_capacity) else {
    return StatusCode::TOO_MANY_REQUESTS.into_response();
  };
  let tracked = state.runtime.tasks.token();
  let limit = state.config.runtime.max_message_bytes;
  configure(ws, limit).on_upgrade(move |socket| async move {
    let _tracked = tracked;
    let _permit = permit;
    tracing::info!(connection_id = %connection.id, "websocket_connected");
    run(socket, auth, connection, state).await;
    tracing::info!("websocket_disconnected");
  })
}

pub async fn send(socket: &mut WebSocket, message: Message, seconds: u64) -> bool {
  matches!(tokio::time::timeout(Duration::from_secs(seconds), socket.send(message)).await, Ok(Ok(())))
}

async fn close(socket: &mut WebSocket, code: u16, reason: &'static str, seconds: u64) {
  if send(socket, Message::Close(Some(CloseFrame { code, reason: reason.into() })), seconds).await {
    let _ = tokio::time::timeout(Duration::from_secs(seconds), async {
      while let Some(Ok(message)) = socket.recv().await {
        if matches!(message, Message::Close(_)) {
          break;
        }
      }
    })
    .await;
  }
}

async fn run(mut socket: WebSocket, auth: AuthUser, mut connection: Connection, state: Arc<AppState>) {
  let config = &state.config.runtime;
  let mut heartbeat = tokio::time::interval(Duration::from_secs((config.idle_seconds / 3).max(1)));
  heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
  let mut last_pong = tokio::time::Instant::now();
  let mut pending_ping = false;
  loop {
    tokio::select! {
      biased;
      _ = state.runtime.shutdown.cancelled() => { close(&mut socket, 1001, "server_shutdown", config.send_seconds).await; break; }
      _ = connection.slow.cancelled() => { close(&mut socket, 1013, "slow_consumer", config.send_seconds).await; break; }
      _ = heartbeat.tick() => {
        if last_pong.elapsed() >= Duration::from_secs(config.idle_seconds) {
          close(&mut socket, 1008, "heartbeat_timeout", config.send_seconds).await; break;
        }
        if !auth.is_current(&state).await {
          close(&mut socket, 1008, "credential_revoked", config.send_seconds).await; break;
        }
        if !send(&mut socket, Message::Ping(Vec::new().into()), config.send_seconds).await { break; }
        pending_ping = true;
      }
      event = connection.receiver.recv() => {
        if let Some(event) = event
          && connection.enabled.load(Ordering::Relaxed)
          && !send(&mut socket, Message::Text(event.to_string().into()), config.send_seconds).await { break; }
      }
      frame = socket.recv() => {
        let Some(Ok(frame)) = frame else { break; };
        if !state.runtime.allow(auth.user_id, config.requests_per_minute) {
          close(&mut socket, 1008, "rate_limit_exceeded", config.send_seconds).await; break;
        }
        match frame {
          Message::Pong(payload) if payload.is_empty() && pending_ping => { last_pong = tokio::time::Instant::now(); pending_ping = false; }
          Message::Ping(payload) => { if !send(&mut socket, Message::Pong(payload), config.send_seconds).await { break; } }
          Message::Close(_) => break,
          Message::Text(text) => {
            let response = match decode(&text) {
              Err(code) => error(None, code),
              Ok(request) => {
                let id = request.id;
                if !auth.is_current(&state).await {
                  close(&mut socket, 1008, "credential_revoked", config.send_seconds).await; break;
                }
                let result = tokio::time::timeout(Duration::from_secs(config.send_seconds), dispatch(&state, &auth, &connection, request.action)).await;
                match result {
                  Ok(Ok(data)) => serde_json::json!({"version":1,"type":"response","id":id,"ok":true,"data":data}),
                  Ok(Err(code)) => error(Some(&id), code),
                  Err(_) => error(Some(&id), "request_timeout"),
                }
              }
            };
            if !send(&mut socket, Message::Text(response.to_string().into()), config.send_seconds).await { break; }
          }
          _ => { close(&mut socket, 1003, "unsupported_frame", config.send_seconds).await; break; }
        }
      }
    }
  }
}

fn error(id: Option<&str>, code: &str) -> serde_json::Value {
  serde_json::json!({"version":1,"type":"error","id":id,"error":{"code":code}})
}

async fn dispatch(state: &AppState, auth: &AuthUser, connection: &Connection, action: Action) -> Result<serde_json::Value, &'static str> {
  // Identity comes exclusively from the authenticated handshake; actions cannot supply an owner/chat.
  match action {
    Action::GetState {} => integration::snapshot(state, auth.user_id).await.and_then(|data| serde_json::to_value(data).map_err(Into::into)).map_err(|_| "storage_unavailable"),
    Action::SetModel { model } => {
      if crate::models::common::LlmModel::from_u8(model).is_none() {
        return Err("invalid_model");
      }
      integration::set_model(state, auth.user_id, model).await.map_err(|_| "storage_unavailable")?;
      Ok(serde_json::json!({"model":model}))
    }
    Action::Subscribe { event } => {
      if event != "profile.updated" {
        return Err("subscription_not_allowed");
      }
      connection.enabled.store(true, Ordering::Relaxed);
      Ok(serde_json::json!({"event":event}))
    }
    Action::Unsubscribe {} => {
      connection.enabled.store(false, Ordering::Relaxed);
      Ok(serde_json::json!({}))
    }
  }
}

/// Bound transport buffers independently of the application notification queue.
pub fn configure(ws: WebSocketUpgrade, message_limit: usize) -> WebSocketUpgrade {
  ws.read_buffer_size(message_limit.min(8192))
    .write_buffer_size(0)
    .max_write_buffer_size(crate::services::llm::MAX_RESPONSE_BYTES * 6 + 4096)
    .max_frame_size(message_limit)
    .max_message_size(message_limit)
}

#[cfg(test)]
mod tests {
  use super::*;
  #[test]
  fn strict_protocol_and_owner_boundary() {
    assert!(decode(r#"{"version":1,"id":"a","action":{"name":"get_state"}}"#).is_ok());
    for bad in [
      r#"{"version":2,"id":"a","action":{"name":"get_state"}}"#,
      r#"{"version":1,"id":"a","action":{"name":"get_state","user_id":2}}"#,
      r#"{"version":1,"id":"","action":{"name":"get_state"}}"#,
      "{}",
      "[]",
      "null",
      "{",
    ] {
      assert!(decode(bad).is_err(), "{bad}");
    }
    let deep = format!("{}0{}", "[".repeat(256), "]".repeat(256));
    assert!(decode(&deep).is_err());
  }
}
