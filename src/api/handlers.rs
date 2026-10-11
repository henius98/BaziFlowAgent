use axum::{
  Json,
  extract::{
    Query,
    ws::{Message, WebSocket, WebSocketUpgrade},
  },
  http::StatusCode,
  response::{
    IntoResponse, Response,
    sse::{Event, Sse},
  },
};
use futures::{StreamExt, stream::Stream};
use serde::Deserialize;
use tracing::error;

use super::auth::AuthUser;
use super::models::*;
use crate::{models, repos, services};

#[derive(Debug, Deserialize, Default)]
pub struct StreamQuery {
  pub stream: Option<bool>,
}

/// Helper to drain an LLM stream receiver into a single string.
async fn collect_stream(mut rx: crate::models::LlmStream) -> crate::models::AppResult<String> {
  let mut bytes = Vec::new();
  while let Some(chunk) = rx.recv().await {
    bytes.extend_from_slice(&chunk);
  }
  rx.finish().await?;
  Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Helper to convert an LLM stream into SSE events.
fn stream_to_sse(mut rx: crate::models::LlmStream) -> impl Stream<Item = Result<Event, std::convert::Infallible>> {
  async_stream::stream! {
      while let Some(chunk) = rx.recv().await {
          let chunk_str = String::from_utf8_lossy(&chunk);
          yield Ok(Event::default().event("chunk").data(chunk_str));
      }
      if rx.finish().await.is_ok() { yield Ok(Event::default().data("[DONE]")); }
      else { yield Ok(Event::default().event("error").data("Analysis interrupted")); }
  }
}

/// Helper to return an error response with status code.
fn api_error(status: StatusCode, msg: impl Into<String>) -> Response {
  (status, Json(ApiError::new(msg))).into_response()
}

// ─────────────────────────────────────────────
// POST /api/v1/profile
// ─────────────────────────────────────────────
pub async fn create_profile(auth: AuthUser, query: Query<StreamQuery>, Json(req): Json<CreateProfileRequest>) -> Response {
  let state = models::get_state();
  let user_id = auth.user_id;
  let is_stream = query.stream.unwrap_or(false);

  // Validate inputs
  if req.gender > 1 {
    return api_error(StatusCode::BAD_REQUEST, "gender must be 0 (female) or 1 (male)");
  }
  if matches!(req.birth_hour, Some(hour) if hour > 23) {
    return api_error(StatusCode::BAD_REQUEST, "birth_hour must be 0-23");
  }
  if matches!(req.birth_minute, Some(minute) if minute > 59) {
    return api_error(StatusCode::BAD_REQUEST, "birth_minute must be 0-59");
  }
  if chrono::NaiveDate::parse_from_str(&req.birth_date, "%Y-%m-%d").is_err() {
    return api_error(StatusCode::BAD_REQUEST, "birth_date must be YYYY-MM-DD format");
  }
  let birth_time = match (req.birth_hour, req.birth_minute) {
    (Some(hour), Some(minute)) => match chrono::NaiveTime::from_hms_opt(hour as u32, minute as u32, 0) {
      Some(time) => Some(time),
      None => return api_error(StatusCode::BAD_REQUEST, "invalid birth time"),
    },
    (None, None) => None,
    _ => {
      return api_error(StatusCode::BAD_REQUEST, "birth_hour and birth_minute must be provided together or both omitted");
    }
  };

  let username = repos::get_username_by_user_id(&state.db_pool, user_id).await.unwrap_or_else(|| "api_user".to_string());
  let user_profile = repos::get_user_profile(&state.db_pool, user_id).await;
  let llm_model = user_profile.llm_model;

  let params = services::bazi_service::DestinyProfileParams { user_id, username: &username, birth_date: &req.birth_date, birth_time, gender: req.gender, location: req.location, llm_model };

  let profile_res = match services::bazi_service::create_destiny_profile(&state, params).await {
    Ok(res) => res,
    Err(e) => {
      error!("API: Failed to create destiny profile: {}", e);
      return api_error(StatusCode::INTERNAL_SERVER_ERROR, "Upstream service unavailable");
    }
  };

  if is_stream {
    let chart_url_clone = profile_res.chart_url.clone();
    let sse_stream = async_stream::stream! {
        // First event: chart URL
        yield Ok::<_, std::convert::Infallible>(Event::default().event("chart_url").data(&chart_url_clone));

        // Stream analysis chunks
        let mut rx = profile_res.stream;
        while let Some(chunk) = rx.recv().await {
            let chunk_str = String::from_utf8_lossy(&chunk);
            yield Ok(Event::default().event("analysis_chunk").data(chunk_str));
        }

        if rx.finish().await.is_ok() {
          yield Ok(Event::default().data("[DONE]"));
        } else {
          yield Ok(Event::default().event("error").data("Analysis interrupted"));
        }
    };
    return Sse::new(sse_stream).into_response();
  }

  // Non-streaming mode: collect full response
  let analysis = match collect_stream(profile_res.stream).await {
    Ok(text) => text,
    Err(_) => return api_error(StatusCode::BAD_GATEWAY, "Analysis interrupted"),
  };

  let user_profile = repos::get_user_profile(&state.db_pool, user_id).await;
  let summary = user_profile.bazi_summary;

  Json(ApiResponse::ok(CreatedProfileData { chart_url: profile_res.chart_url, bazi_analysis: analysis, bazi_summary: summary })).into_response()
}

// ─────────────────────────────────────────────
// GET /api/v1/profile
// ─────────────────────────────────────────────
pub async fn get_profile(auth: AuthUser) -> Response {
  let state = models::get_state();
  let user_id = auth.user_id;
  let user_profile = repos::get_user_profile(&state.db_pool, user_id).await;

  if user_profile.bazi_four_pillars.is_none() {
    return api_error(StatusCode::NOT_FOUND, "No Bazi profile found. Create one first via POST /api/v1/profile.");
  }

  let parsed = services::paipan::models::StructuredBazi::from_json(user_profile.bazi_four_pillars.as_deref()).ok();

  let (gender_str, solar_date, lunar_date, birth_time_known, birth_location, pillars_json) = match &parsed {
    Some(structured) => (
      Some(structured.info.gender.clone()),
      Some(structured.info.solar_date.clone()),
      Some(structured.info.lunisolar_date.clone()),
      Some(structured.info.birth_time_known),
      (structured.info.birth_location != "未知").then(|| structured.info.birth_location.clone()),
      serde_json::to_value(&structured.pillars).ok(),
    ),
    None => (None, None, None, None, None, None),
  };

  let chart_url = services::bazi_service::get_bazi_chart_url(&state, user_id).await.ok();

  let llm_model_str = user_profile.llm_model.map(|m| m.as_str().to_string());

  Json(ApiResponse::ok(ProfileData {
    profile: ProfileDetail {
      gender: gender_str,
      solar_date,
      lunar_date,
      birth_time_known,
      birth_location,
      pillars: pillars_json,
      chart_url,
      bazi_analysis: user_profile.bazi_analysis,
      bazi_summary: user_profile.bazi_summary,
      llm_model: llm_model_str,
      schedule: user_profile.schedule,
    },
  }))
  .into_response()
}

// ─────────────────────────────────────────────
// POST /api/v1/date-fortune
// ─────────────────────────────────────────────
pub async fn date_fortune(auth: AuthUser, headers: axum::http::HeaderMap, ws: WebSocketUpgrade) -> Response {
  let state = models::get_state();
  if !super::gateway::origin_allowed(&headers, &state.config.cors_allowed_origin) {
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
  super::gateway::configure(ws, limit).on_upgrade(move |socket| async move {
    let _tracked = tracked;
    let _permit = permit;
    let _connection = connection;
    tokio::select! {
      _ = state.runtime.shutdown.cancelled() => {},
      _ = tokio::time::sleep(std::time::Duration::from_secs(state.config.runtime.work_seconds)) => {},
      result = handle_socket(socket, auth) => if let Err(e) = result {
      tracing::error!("WebSocket connection error: {}", e);
      }
    }
  })
}

async fn send_ws_msg(socket: &mut WebSocket, msg: &ServerMessage) -> crate::models::AppResult<()> {
  let text = serde_json::to_string(msg)?;
  if !super::gateway::send(socket, Message::Text(text.into()), models::get_state().config.runtime.send_seconds).await {
    return Err(anyhow::anyhow!("WebSocket send timed out or closed").into());
  }
  Ok(())
}

async fn handle_socket(mut socket: WebSocket, auth: AuthUser) -> crate::models::AppResult<()> {
  let state = models::get_state();

  let user_id = auth.user_id;
  // 1. Wait for Generate message, with a bounded initialization deadline.
  let (req_date, req_effort) = match tokio::time::timeout(std::time::Duration::from_secs(state.config.runtime.idle_seconds), socket.next()).await.ok().flatten() {
    Some(Ok(Message::Text(text))) => match serde_json::from_str::<ClientMessage>(&text) {
      Ok(ClientMessage::Generate { date, reasoning_effort }) => {
        let effort = match reasoning_effort {
          Some(e) => match models::ReasoningEffortLevel::parse(&e) {
            Some(level) => Some(level),
            None => {
              send_ws_msg(&mut socket, &ServerMessage::Error { message: "Invalid reasoning_effort: accepted values are none, minimal, low, medium, high, xhigh".into() }).await?;
              return Ok(());
            }
          },
          None => None,
        };
        (date, effort)
      }
      _ => {
        send_ws_msg(&mut socket, &ServerMessage::Error { message: "Expected Generate action".into() }).await?;
        return Ok(());
      }
    },
    _ => return Ok(()),
  };

  if !auth.is_current(&state).await {
    send_ws_msg(&mut socket, &ServerMessage::Error { message: "Credential revoked".into() }).await?;
    return Ok(());
  }
  if chrono::NaiveDate::parse_from_str(&req_date, "%Y-%m-%d").is_err() {
    send_ws_msg(&mut socket, &ServerMessage::Error { message: "Invalid date".into() }).await?;
    return Ok(());
  }
  let Some(_guard) = crate::models::ProcessingGuard::acquire(state.clone(), user_id) else {
    send_ws_msg(&mut socket, &ServerMessage::Error { message: "Service busy".into() }).await?;
    return Ok(());
  };
  let fortune_result = match services::almanac::get_daily_fortune(&state, user_id, &req_date, true, models::LlmRequestType::ApiDateFortune, req_effort).await {
    Ok(res) => res,
    Err(models::AppError::ProfileNotFound) => {
      send_ws_msg(&mut socket, &ServerMessage::Error { message: "No Bazi profile found".into() }).await?;
      return Ok(());
    }
    Err(_e) => {
      send_ws_msg(&mut socket, &ServerMessage::Error { message: "Almanac service unavailable".into() }).await?;
      return Ok(());
    }
  };

  // Send almanac data
  let almanac_val = serde_json::to_value(&fortune_result.almanac_data)?;
  send_ws_msg(&mut socket, &ServerMessage::Almanac { data: almanac_val }).await?;

  match fortune_result.response {
    models::LlmResponse::Stream(mut receiver) => {
      let mut credential_check = tokio::time::interval(std::time::Duration::from_secs((state.config.runtime.idle_seconds / 3).max(1)));
      credential_check.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
      loop {
        tokio::select! {
            _ = credential_check.tick() => {
              if !auth.is_current(&state).await {
                send_ws_msg(&mut socket, &ServerMessage::Error { message: "Credential revoked".into() }).await?;
                break;
              }
            }
            msg = socket.next() => {
                match msg {
                    Some(Ok(Message::Close(_))) => break,
                    Some(Ok(Message::Text(text))) => {
                        if !state.runtime.allow(user_id, auth.rate_limit) || !auth.is_current(&state).await { break; }
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
                        send_ws_msg(&mut socket, &ServerMessage::Chunk { data: String::from_utf8_lossy(&c).into_owned() }).await?;
                    }
                    None => {
                        if receiver.finish().await.is_ok() { send_ws_msg(&mut socket, &ServerMessage::Done).await?; }
                        else { send_ws_msg(&mut socket, &ServerMessage::Error { message: "Analysis interrupted".into() }).await?; }
                        break;
                    }
                }
            }
        }
      }
    }
    _ => {
      send_ws_msg(&mut socket, &ServerMessage::Error { message: "Expected stream".into() }).await?;
    }
  }

  Ok(())
}

// ─────────────────────────────────────────────
// POST /api/v1/pick-date
// ─────────────────────────────────────────────
pub async fn pick_date(auth: AuthUser, query: Query<StreamQuery>, Json(req): Json<PickDateRequest>) -> Response {
  let state = models::get_state();
  let user_id = auth.user_id;
  let is_stream = query.stream.unwrap_or(false);

  // Validate dates
  if chrono::NaiveDate::parse_from_str(&req.start_date, "%Y-%m-%d").is_err() {
    return api_error(StatusCode::BAD_REQUEST, "start_date must be YYYY-MM-DD format");
  }
  if chrono::NaiveDate::parse_from_str(&req.end_date, "%Y-%m-%d").is_err() {
    return api_error(StatusCode::BAD_REQUEST, "end_date must be YYYY-MM-DD format");
  }
  if req.activity.trim().is_empty() {
    return api_error(StatusCode::BAD_REQUEST, "activity must not be empty");
  }

  let reasoning_effort = match req.reasoning_effort.as_deref() {
    Some(e) => match models::ReasoningEffortLevel::parse(e) {
      Some(level) => Some(level),
      None => return api_error(StatusCode::BAD_REQUEST, "Invalid reasoning_effort: accepted values are none, minimal, low, medium, high, xhigh"),
    },
    None => None,
  };

  let params = services::almanac::DateSelectionParams {
    user_id,
    start_date: &req.start_date,
    end_date: &req.end_date,
    activity: &req.activity,
    stream: is_stream,
    request_type: models::LlmRequestType::ApiPickDate,
    reasoning_effort,
  };

  match services::almanac::pick_dates(&state, params).await {
    Ok(result) => match result.response {
      models::LlmResponse::Stream(receiver) if is_stream => Sse::new(stream_to_sse(receiver)).into_response(),
      models::LlmResponse::Full(analysis) => Json(ApiResponse::ok(PickData { analysis })).into_response(),
      _ => api_error(StatusCode::INTERNAL_SERVER_ERROR, "Unexpected response type from LLM"),
    },
    Err(models::AppError::ProfileNotFound) => api_error(StatusCode::BAD_REQUEST, "No Bazi profile found. Create one first via POST /api/v1/profile."),
    Err(models::AppError::Message(msg)) => api_error(StatusCode::BAD_REQUEST, msg),
    Err(e) => {
      error!("API: Pick date error: {}", e);
      api_error(StatusCode::INTERNAL_SERVER_ERROR, "Upstream service unavailable")
    }
  }
}

// ─────────────────────────────────────────────
// GET /api/v1/models
// ─────────────────────────────────────────────
pub async fn get_models(_auth: AuthUser) -> Response {
  let state = models::get_state();
  match repos::get_llm_models(&state.db_pool).await {
    Ok(models) => Json(ApiResponse::ok(ModelsData { models })).into_response(),
    Err(e) => {
      error!("API: Get models error: {}", e);
      api_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to fetch models")
    }
  }
}

// ─────────────────────────────────────────────
// PUT /api/v1/model
// ─────────────────────────────────────────────
pub async fn update_model(auth: AuthUser, Json(req): Json<UpdateModelRequest>) -> Response {
  let state = models::get_state();
  let user_id = auth.user_id;

  let model = match services::integration::set_model(&state, user_id, req.model).await {
    Ok(model) => model,
    Err(models::AppError::Message(_)) => return api_error(StatusCode::BAD_REQUEST, "Model is unavailable"),
    Err(_) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to save model"),
  };

  Json(ApiResponse::ok(ModelData { model: model.as_str().to_string() })).into_response()
}

// ─────────────────────────────────────────────
// PUT /api/v1/schedule
// ─────────────────────────────────────────────
pub async fn update_schedule(auth: AuthUser, Json(req): Json<UpdateScheduleRequest>) -> Response {
  let state = models::get_state();
  let user_id = auth.user_id;

  let schedule_val = match &req.time {
    Some(time_str) => {
      // Parse HH:MM format
      let parts: Vec<&str> = time_str.split(':').collect();
      if parts.len() != 2 {
        return api_error(StatusCode::BAD_REQUEST, "time must be HH:MM format");
      }
      let hour: u32 = match parts[0].parse() {
        Ok(h) if h < 24 => h,
        _ => return api_error(StatusCode::BAD_REQUEST, "Invalid hour in time (0-23)"),
      };
      let minute: u32 = match parts[1].parse() {
        Ok(m) if m < 60 => m,
        _ => return api_error(StatusCode::BAD_REQUEST, "Invalid minute in time (0-59)"),
      };
      Some(format!("0 {} {} * * * *", minute, hour))
    }
    None => None,
  };

  if let Err(e) = repos::update_user_schedule(&state.db_pool, user_id, schedule_val.as_deref()).await {
    error!("API: Failed to update schedule: {}", e);
    return api_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to update schedule");
  }

  // Schedule runtime updates use the initialized scheduler's shared bot instance.
  // The durable schedule is restored on restart as well as updated immediately.
  // Telegram /schedule and HTTP settings therefore use the same scheduler.
  if let Some(bot) = crate::scheduler::GLOBAL_BOT.get() {
    match &schedule_val {
      Some(cron) => crate::scheduler::add_or_update_user_schedule(bot.clone(), user_id, cron).await,
      None => crate::scheduler::remove_user_daily_job(user_id).await,
    }
  }

  Json(ApiResponse::ok(ScheduleData { schedule: req.time })).into_response()
}

// ─────────────────────────────────────────────
// POST /api/v1/chat
// ─────────────────────────────────────────────
pub async fn chat(auth: AuthUser, query: Query<StreamQuery>, Json(req): Json<ChatRequest>) -> Response {
  let state = models::get_state();
  let user_id = auth.user_id;
  let is_stream = query.stream.unwrap_or(false);

  if req.message.trim().is_empty() {
    return api_error(StatusCode::BAD_REQUEST, "message must not be empty");
  }

  let reasoning_effort = match req.reasoning_effort.as_deref() {
    Some(e) => match models::ReasoningEffortLevel::parse(e) {
      Some(level) => Some(level),
      None => return api_error(StatusCode::BAD_REQUEST, "Invalid reasoning_effort: accepted values are none, minimal, low, medium, high, xhigh"),
    },
    None => None,
  };

  let response = match services::conversation::submit_chat_turn(&state, user_id, &req.message, is_stream, models::LlmRequestType::ApiChat, reasoning_effort).await {
    Ok(res) => res,
    Err(models::AppError::ProfileNotFound) => {
      return api_error(StatusCode::BAD_REQUEST, "No Bazi profile found. Create one first via POST /api/v1/profile.");
    }
    Err(e) => {
      error!("API: Chat error: {}", e);
      return api_error(StatusCode::INTERNAL_SERVER_ERROR, "Upstream service unavailable");
    }
  };

  match response {
    models::LlmResponse::Stream(mut rx) => {
      let sse_stream = async_stream::stream! {
        while let Some(chunk) = rx.recv().await {
          let chunk_str = String::from_utf8_lossy(&chunk);
          yield Ok::<_, std::convert::Infallible>(Event::default().event("chunk").data(chunk_str));
        }
        if rx.finish().await.is_ok() {
          yield Ok(Event::default().data("[DONE]"));
        } else {
          yield Ok(Event::default().event("error").data("Analysis interrupted"));
        }
      };
      Sse::new(sse_stream).into_response()
    }
    models::LlmResponse::Full(reply) => Json(ApiResponse::ok(ChatData { reply })).into_response(),
  }
}
