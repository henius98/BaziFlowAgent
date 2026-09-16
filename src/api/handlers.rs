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
  let mut result = String::new();
  while let Some(chunk) = rx.recv().await {
    result.push_str(&chunk);
  }
  rx.finish().await?;
  Ok(result)
}

/// Helper to convert an LLM stream into SSE events.
fn stream_to_sse(mut rx: crate::models::LlmStream) -> impl Stream<Item = Result<Event, std::convert::Infallible>> {
  async_stream::stream! {
      while let Some(chunk) = rx.recv().await {
          yield Ok(Event::default().event("chunk").data(chunk));
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

  let params = services::bazi_service::BaziDataParams { user_id, username: &username, birth_date: &req.birth_date, birth_time, gender: req.gender, location: req.location };

  let structured_data = match services::bazi_service::prepare_bazi_data(&state, params).await {
    Ok(data) => data,
    Err(e) => {
      error!("API: Failed to prepare bazi data: {}", e);
      return api_error(StatusCode::INTERNAL_SERVER_ERROR, "Upstream service unavailable");
    }
  };
  let structured_bazi = structured_data.to_string();

  // A new chart invalidates every conversation turn derived from the old profile.
  {
    let mut ctx = state.user_contexts.entry(user_id).or_default();
    ctx.messages.clear();
    ctx.history_loaded = true;
    ctx.last_active = chrono::Utc::now();
  }
  if let Err(e) = state.chat_cache_writer.clear(user_id).await {
    error!("API: Failed to clear stale chat history for user {}: {}", user_id, e);
  }

  // Build and save HTML chart
  services::bazi_service::build_and_save_bazi_html(&state, user_id, &username, &structured_data).await;

  let chart_url = services::bazi_service::get_bazi_chart_url(&state, user_id).await.unwrap_or_else(|_| format!("{}/bazi_{}.html", state.config.base_url.trim_end_matches('/'), user_id));

  let user_profile = repos::get_user_profile(&state.db_pool, user_id).await;
  let llm_model = user_profile.llm_model;

  if is_stream {
    let structured_bazi_for_stream = structured_bazi.clone();
    // SSE streaming mode
    match services::bazi_service::core_bazi_analysis(&state, user_id, &structured_data, llm_model).await {
      Ok(receiver) => {
        let chart_url_clone = chart_url.clone();
        let sse_stream = async_stream::stream! {
            // First event: chart URL
            yield Ok::<_, std::convert::Infallible>(Event::default().event("chart_url").data(&chart_url_clone));

            // Stream analysis chunks
            let mut rx = receiver;
            let mut full_text = String::new();
            while let Some(chunk) = rx.recv().await {
                full_text.push_str(&chunk);
                yield Ok(Event::default().event("analysis_chunk").data(&chunk));
            }

            if rx.finish().await.is_err() {
              yield Ok(Event::default().event("error").data("Analysis interrupted"));
              return;
            }
            // Save analysis to DB and generate summary before declaring completion.
            if !full_text.is_empty() {
                let state = models::get_state();
                async move {
                    repos::save_user_bazi_analysis(&state.db_pool, user_id, &full_text).await;

                    if let Ok(summary) = services::bazi_service::generate_bazi_summary(
                        &state,
                        user_id,
                        &structured_bazi_for_stream,
                        &full_text,
                        llm_model,
                    ).await {
                        repos::save_user_bazi_summary(&state.db_pool, user_id, &summary).await;
                    }
                }.await;
            }
            yield Ok(Event::default().data("[DONE]"));
        };
        return Sse::new(sse_stream).into_response();
      }
      Err(e) => {
        error!("API: Failed to start bazi analysis stream: {}", e);
        return api_error(StatusCode::INTERNAL_SERVER_ERROR, "Upstream service unavailable");
      }
    }
  }

  // Non-streaming mode: collect full response
  let analysis = match services::bazi_service::core_bazi_analysis(&state, user_id, &structured_data, llm_model).await {
    Ok(receiver) => match collect_stream(receiver).await {
      Ok(text) => text,
      Err(_) => return api_error(StatusCode::BAD_GATEWAY, "Analysis interrupted"),
    },
    Err(e) => {
      error!("API: Failed to generate bazi analysis: {}", e);
      return api_error(StatusCode::INTERNAL_SERVER_ERROR, "Upstream service unavailable");
    }
  };

  if !analysis.is_empty() {
    repos::save_user_bazi_analysis(&state.db_pool, user_id, &analysis).await;
  }

  // Generate summary
  let summary = if !analysis.is_empty() {
    match services::bazi_service::generate_bazi_summary(&state, user_id, &structured_bazi, &analysis, llm_model).await {
      Ok(s) => {
        repos::save_user_bazi_summary(&state.db_pool, user_id, &s).await;
        Some(s)
      }
      Err(e) => {
        error!("API: Failed to generate bazi summary: {}", e);
        None
      }
    }
  } else {
    None
  };

  Json(ApiResponse::ok(CreatedProfileData { chart_url, bazi_analysis: analysis, bazi_summary: summary })).into_response()
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

  let bazi_raw = user_profile.bazi_four_pillars.as_deref().unwrap_or("");
  let parsed = crate::utils::parse_user_bazi(Some(bazi_raw)).ok();

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
  let req_date = match tokio::time::timeout(std::time::Duration::from_secs(state.config.runtime.idle_seconds), socket.next()).await.ok().flatten() {
    Some(Ok(Message::Text(text))) => match serde_json::from_str::<ClientMessage>(&text) {
      Ok(ClientMessage::Generate { date }) => date,
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
  let user_profile = repos::get_user_profile(&state.db_pool, user_id).await;
  let bazi_four_pillars = user_profile.bazi_four_pillars.as_deref().and_then(|raw| serde_json::from_str::<services::paipan::StructuredBazi>(raw).ok()).map(|b| b.to_string());

  let Some(bazi_four_pillars) = bazi_four_pillars.as_deref() else {
    send_ws_msg(&mut socket, &ServerMessage::Error { message: "No Bazi profile found".into() }).await?;
    return Ok(());
  };

  let almanac_data = match services::almanac::fetch_and_format_almanac(&state.http_client, &req_date).await {
    Ok(data) => data,
    Err(_e) => {
      send_ws_msg(&mut socket, &ServerMessage::Error { message: "Almanac service unavailable".into() }).await?;
      return Ok(());
    }
  };

  // Send almanac data
  let almanac_val = serde_json::to_value(&almanac_data)?;
  send_ws_msg(&mut socket, &ServerMessage::Almanac { data: almanac_val }).await?;

  let bazi_summary = user_profile.bazi_summary.as_deref().unwrap_or_else(|| user_profile.bazi_analysis.as_deref().unwrap_or_default());

  let llm_result = services::almanac::analysis_date_fortune(services::almanac::DateFortuneRequest {
    target_date: &req_date,
    almanac_data: &almanac_data,
    bazi_four_pillars,
    bazi_summary,
    stream: true,
    llm_model: user_profile.llm_model,
    user_id: Some(user_id as i64),
    request_type: Some(models::LlmRequestType::ApiDateFortune),
  })
  .await;

  match llm_result {
    Ok(models::LlmResponse::Stream(mut receiver)) => {
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
                        if !state.runtime.allow(user_id, state.config.runtime.requests_per_minute) || !auth.is_current(&state).await { break; }
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
                        send_ws_msg(&mut socket, &ServerMessage::Chunk { data: c }).await?;
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

  let user_profile = repos::get_user_profile(&state.db_pool, user_id).await;
  let bazi_four_pillars = user_profile.bazi_four_pillars.as_deref().and_then(|raw| serde_json::from_str::<services::paipan::StructuredBazi>(raw).ok()).map(|b| b.to_string());

  let Some(bazi_four_pillars) = bazi_four_pillars.as_deref() else {
    return api_error(StatusCode::BAD_REQUEST, "No Bazi profile found. Create one first via POST /api/v1/profile.");
  };

  let bazi_summary = user_profile.bazi_summary.as_deref().unwrap_or_else(|| user_profile.bazi_analysis.as_deref().unwrap_or_default());

  match services::almanac::analysis_pick_selection(services::almanac::PickSelectionRequest {
    start_date: &req.start_date,
    end_date: &req.end_date,
    activity: &req.activity,
    bazi_four_pillars,
    bazi_summary,
    stream: is_stream,
    llm_model: user_profile.llm_model,
    user_id: Some(user_id as i64),
    request_type: Some(models::LlmRequestType::ApiPickDate),
  })
  .await
  {
    Ok(models::LlmResponse::Stream(receiver)) if is_stream => Sse::new(stream_to_sse(receiver)).into_response(),
    Ok(models::LlmResponse::Full(analysis)) => Json(ApiResponse::ok(PickData { analysis })).into_response(),
    Ok(_) => api_error(StatusCode::INTERNAL_SERVER_ERROR, "Unexpected response type from LLM"),
    Err(e) => {
      error!("API: Pick date error: {}", e);
      api_error(StatusCode::INTERNAL_SERVER_ERROR, "Upstream service unavailable")
    }
  }
}

// ─────────────────────────────────────────────
// PUT /api/v1/model
// ─────────────────────────────────────────────
pub async fn update_model(auth: AuthUser, Json(req): Json<UpdateModelRequest>) -> Response {
  let state = models::get_state();
  let user_id = auth.user_id;

  let model = match models::common::LlmModel::from_u8(req.model) {
    Some(m) => m,
    None => {
      return api_error(StatusCode::BAD_REQUEST, format!("Invalid model ID: {}. Valid: 0, 1, 2", req.model));
    }
  };

  if services::integration::set_model(&state, user_id, req.model).await.is_err() {
    return api_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to save model");
  }

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

  let user_profile = repos::get_user_profile(&state.db_pool, user_id).await;
  let Some(bazi_four_pillars) = user_profile.bazi_four_pillars.as_deref() else {
    return api_error(StatusCode::BAD_REQUEST, "No Bazi profile found. Create one first via POST /api/v1/profile.");
  };
  let bazi_context = match crate::utils::build_bazi_context_message(bazi_four_pillars, user_profile.bazi_summary.as_deref()) {
    Ok(message) => message,
    Err(e) => {
      error!("API: Failed to build Bazi context for user {}: {}", user_id, e);
      return api_error(StatusCode::INTERNAL_SERVER_ERROR, "Invalid Bazi profile");
    }
  };

  // Restore from SQLite only after a memory expiry or process restart.
  crate::utils::hydrate_chat_context(&state, user_id).await;

  // Push user message to the in-memory fast path.
  {
    let mut ctx = state.user_contexts.entry(user_id).or_default();
    ctx.push_message(format!("User: {}", req.message), state.config.max_context_messages);
    ctx.last_active = chrono::Utc::now();
  }
  if let Err(e) = state.chat_cache_writer.save(user_id, "user", &req.message) {
    error!("API: Failed to cache chat message for user {}: {}", user_id, e);
  }

  let system_prompt_text = include_str!("../../prompts/FollowUpAssistant.md");
  let system_msg = match async_openai::types::chat::ChatCompletionRequestSystemMessageArgs::default().content(system_prompt_text).build() {
    Ok(m) => m,
    Err(e) => {
      error!("API: Failed to build system message: {}", e);
      return api_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to build prompt");
    }
  };

  let mut messages: Vec<async_openai::types::chat::ChatCompletionRequestMessage> = vec![system_msg.into()];
  messages.push(bazi_context);

  if let Some(ctx) = state.user_contexts.get(&user_id) {
    messages.extend(crate::utils::build_chat_messages(&ctx.messages));
  }

  let model_name = user_profile.llm_model.map(|m| m.as_str().to_string()).unwrap_or_else(|| state.config.llm_model_name.clone());

  let mut params = services::llm::LlmRequestParams::new(model_name, messages);
  params.stream = Some(is_stream);
  params.temperature = Some(0.4);
  params.user_id = Some(user_id as i64);
  params.request_type = Some(models::LlmRequestType::ApiChat);

  match state.llm_service.call(params).await {
    Ok(models::LlmResponse::Stream(mut receiver)) if is_stream => {
      let sse_stream = async_stream::stream! {
        let mut full_reply = String::new();
        while let Some(chunk) = receiver.recv().await {
          full_reply.push_str(&chunk);
          yield Ok::<_, std::convert::Infallible>(Event::default().event("chunk").data(chunk));
        }
        if receiver.finish().await.is_err() {
          yield Ok(Event::default().event("error").data("Analysis interrupted"));
          return;
        }
        // Save assistant response to context after stream completes.
        {
          let mut ctx = state.user_contexts.entry(user_id).or_default();
          ctx.push_message(format!("Assistant: {}", full_reply), state.config.max_context_messages);
        }
        if let Err(e) = state.chat_cache_writer.save(user_id, "assistant", &full_reply) {
          error!("API: Failed to cache assistant response for user {}: {}", user_id, e);
        }
        yield Ok(Event::default().data("[DONE]"));
      };
      Sse::new(sse_stream).into_response()
    }

    Ok(models::LlmResponse::Full(reply)) => {
      // Save assistant response to context
      {
        let mut ctx = state.user_contexts.entry(user_id).or_default();
        ctx.push_message(format!("Assistant: {}", reply), state.config.max_context_messages);
      }
      if let Err(e) = state.chat_cache_writer.save(user_id, "assistant", &reply) {
        error!("API: Failed to cache assistant response for user {}: {}", user_id, e);
      }
      Json(ApiResponse::ok(ChatData { reply })).into_response()
    }
    Ok(models::LlmResponse::Stream(receiver)) => {
      // Streaming returned but not requested — collect it
      let reply = match collect_stream(receiver).await {
        Ok(reply) => reply,
        Err(_) => return api_error(StatusCode::BAD_GATEWAY, "Analysis interrupted"),
      };
      {
        let mut ctx = state.user_contexts.entry(user_id).or_default();
        ctx.push_message(format!("Assistant: {}", reply), state.config.max_context_messages);
      }
      if let Err(e) = state.chat_cache_writer.save(user_id, "assistant", &reply) {
        error!("API: Failed to cache assistant response for user {}: {}", user_id, e);
      }
      Json(ApiResponse::ok(ChatData { reply })).into_response()
    }
    Err(e) => {
      error!("API: Chat error: {}", e);
      api_error(StatusCode::INTERNAL_SERVER_ERROR, "Upstream service unavailable")
    }
  }
}
