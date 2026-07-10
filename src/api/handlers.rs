use axum::{
    Json,
    extract::Query,
    http::StatusCode,
    response::{
        IntoResponse, Response,
        sse::{Event, Sse},
    },
};
use futures::stream::Stream;
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
async fn collect_stream(mut rx: tokio::sync::mpsc::Receiver<String>) -> String {
    let mut result = String::new();
    while let Some(chunk) = rx.recv().await {
        result.push_str(&chunk);
    }
    result
}

/// Helper to convert an LLM stream into SSE events.
fn stream_to_sse(
    mut rx: tokio::sync::mpsc::Receiver<String>,
) -> impl Stream<Item = Result<Event, std::convert::Infallible>> {
    async_stream::stream! {
        while let Some(chunk) = rx.recv().await {
            yield Ok(Event::default().event("chunk").data(chunk));
        }
        yield Ok(Event::default().data("[DONE]"));
    }
}

/// Helper to return an error response with status code.
fn api_error(status: StatusCode, msg: impl Into<String>) -> Response {
    (status, Json(ApiError::new(msg))).into_response()
}

// ─────────────────────────────────────────────
// POST /api/v1/profile
// ─────────────────────────────────────────────
pub async fn create_profile(
    auth: AuthUser,
    query: Query<StreamQuery>,
    Json(req): Json<CreateProfileRequest>,
) -> Response {
    let state = models::get_state();
    let user_id = auth.user_id;
    let is_stream = query.stream.unwrap_or(false);

    // Validate inputs
    if req.gender > 1 {
        return api_error(
            StatusCode::BAD_REQUEST,
            "gender must be 0 (female) or 1 (male)",
        );
    }
    if req.birth_hour > 23 {
        return api_error(StatusCode::BAD_REQUEST, "birth_hour must be 0-23");
    }
    if req.birth_minute > 59 {
        return api_error(StatusCode::BAD_REQUEST, "birth_minute must be 0-59");
    }
    if chrono::NaiveDate::parse_from_str(&req.birth_date, "%Y-%m-%d").is_err() {
        return api_error(
            StatusCode::BAD_REQUEST,
            "birth_date must be YYYY-MM-DD format",
        );
    }

    let username = repos::get_username_by_user_id(&state.db_pool, user_id)
        .await
        .unwrap_or_else(|| "api_user".to_string());

    let params = services::bazi_service::BaziDataParams {
        user_id,
        username: &username,
        birth_date: &req.birth_date,
        birth_hour: req.birth_hour,
        birth_minute: req.birth_minute,
        gender: req.gender,
        location: req.location,
    };

    let structured_data = match services::bazi_service::prepare_bazi_data(&state, params).await {
        Ok(data) => data,
        Err(e) => {
            error!("API: Failed to prepare bazi data: {}", e);
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to generate Bazi chart: {}", e),
            );
        }
    };

    // Build and save HTML chart
    services::bazi_service::build_and_save_bazi_html(&state, user_id, &username, &structured_data)
        .await;

    let chart_url = services::bazi_service::get_bazi_chart_url(&state, user_id)
        .await
        .unwrap_or_else(|_| {
            format!(
                "{}/bazi_{}.html",
                state.config.base_url.trim_end_matches('/'),
                user_id
            )
        });

    let user_profile = repos::get_user_profile(&state.db_pool, user_id).await;
    let llm_model = user_profile.llm_model;

    if is_stream {
        // SSE streaming mode
        match services::bazi_service::core_bazi_analysis(
            &state,
            user_id,
            &structured_data,
            llm_model,
        )
        .await
        {
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

                    yield Ok(Event::default().data("[DONE]"));

                    // Save analysis to DB and generate summary in background
                    if !full_text.is_empty() {
                        let state = models::get_state();
                        tokio::spawn(async move {
                            repos::save_user_bazi_analysis(&state.db_pool, user_id, &full_text).await;

                            if let Ok(summary) = services::bazi_service::generate_bazi_summary(
                                &state, user_id, &full_text, llm_model,
                            ).await {
                                repos::save_user_bazi_summary(&state.db_pool, user_id, &summary).await;
                            }
                        });
                    }
                };
                return Sse::new(sse_stream).into_response();
            }
            Err(e) => {
                error!("API: Failed to start bazi analysis stream: {}", e);
                return api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to generate analysis: {}", e),
                );
            }
        }
    }

    // Non-streaming mode: collect full response
    let analysis = match services::bazi_service::core_bazi_analysis(
        &state,
        user_id,
        &structured_data,
        llm_model,
    )
    .await
    {
        Ok(receiver) => collect_stream(receiver).await,
        Err(e) => {
            error!("API: Failed to generate bazi analysis: {}", e);
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to generate analysis: {}", e),
            );
        }
    };

    if !analysis.is_empty() {
        repos::save_user_bazi_analysis(&state.db_pool, user_id, &analysis).await;
    }

    // Generate summary
    let summary = if !analysis.is_empty() {
        match services::bazi_service::generate_bazi_summary(&state, user_id, &analysis, llm_model)
            .await
        {
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

    Json(ApiResponse::ok(CreatedProfileData {
        chart_url,
        bazi_analysis: analysis,
        bazi_summary: summary,
    }))
    .into_response()
}

// ─────────────────────────────────────────────
// GET /api/v1/profile
// ─────────────────────────────────────────────
pub async fn get_profile(auth: AuthUser) -> Response {
    let state = models::get_state();
    let user_id = auth.user_id;
    let user_profile = repos::get_user_profile(&state.db_pool, user_id).await;

    if user_profile.bazi_four_pillars.is_none() {
        return api_error(
            StatusCode::NOT_FOUND,
            "No Bazi profile found. Create one first via POST /api/v1/profile.",
        );
    }

    let bazi_raw = user_profile.bazi_four_pillars.as_deref().unwrap_or("");
    let parsed: Option<services::paipan::StructuredBazi> = serde_json::from_str(bazi_raw).ok();

    let (gender_str, solar_date, lunar_date, pillars_json) = match &parsed {
        Some(structured) => (
            Some(structured.info.gender.clone()),
            Some(structured.info.solar_date.clone()),
            Some(structured.info.lunisolar_date.clone()),
            serde_json::to_value(&structured.pillars).ok(),
        ),
        None => (None, None, None, None),
    };

    let chart_url = services::bazi_service::get_bazi_chart_url(&state, user_id)
        .await
        .ok();

    let llm_model_str = user_profile.llm_model.map(|m| m.as_str().to_string());

    Json(ApiResponse::ok(ProfileData {
        profile: ProfileDetail {
            gender: gender_str,
            solar_date,
            lunar_date,
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
pub async fn date_fortune(
    auth: AuthUser,
    query: Query<StreamQuery>,
    Json(req): Json<DateFortuneRequest>,
) -> Response {
    let state = models::get_state();
    let user_id = auth.user_id;
    let is_stream = query.stream.unwrap_or(false);

    if chrono::NaiveDate::parse_from_str(&req.date, "%Y-%m-%d").is_err() {
        return api_error(StatusCode::BAD_REQUEST, "date must be YYYY-MM-DD format");
    }

    let user_profile = repos::get_user_profile(&state.db_pool, user_id).await;
    let bazi_four_pillars = user_profile
        .bazi_four_pillars
        .as_deref()
        .and_then(|raw| serde_json::from_str::<services::paipan::StructuredBazi>(raw).ok())
        .map(|b| b.to_string());

    let Some(bazi_four_pillars) = bazi_four_pillars.as_deref() else {
        return api_error(
            StatusCode::BAD_REQUEST,
            "No Bazi profile found. Create one first via POST /api/v1/profile.",
        );
    };

    let almanac_data =
        match services::almanac::fetch_and_format_almanac(&state.http_client, &req.date).await {
            Ok(data) => data,
            Err(e) => {
                error!("API: Failed to fetch almanac: {}", e);
                return api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to fetch almanac data: {}", e),
                );
            }
        };

    let bazi_summary = user_profile
        .bazi_summary
        .as_deref()
        .unwrap_or_else(|| user_profile.bazi_analysis.as_deref().unwrap_or_default());

    match services::almanac::analysis_date_fortune(services::almanac::DateFortuneRequest {
        target_date: &req.date,
        almanac_data: &almanac_data,
        bazi_four_pillars,
        bazi_summary,
        stream: is_stream,
        llm_model: user_profile.llm_model,
        user_id: Some(user_id as i64),
        request_type: Some("api_date_fortune".to_string()),
    })
    .await
    {
        Ok(models::LlmResponse::Stream(receiver)) if is_stream => {
            let almanac_clone = serde_json::to_string(&almanac_data).unwrap_or_default();
            let sse_stream = async_stream::stream! {
                yield Ok::<_, std::convert::Infallible>(Event::default().event("almanac").data(almanac_clone));
                let mut rx = receiver;
                while let Some(chunk) = rx.recv().await {
                    yield Ok(Event::default().event("analysis_chunk").data(chunk));
                }
                yield Ok(Event::default().data("[DONE]"));
            };
            Sse::new(sse_stream).into_response()
        }
        Ok(models::LlmResponse::Full(analysis)) => Json(ApiResponse::ok(FortuneData {
            almanac: almanac_data,
            analysis,
        }))
        .into_response(),
        Ok(_) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unexpected response type from LLM",
        ),
        Err(e) => {
            error!("API: Date fortune error: {}", e);
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to generate fortune analysis: {}", e),
            )
        }
    }
}

// ─────────────────────────────────────────────
// POST /api/v1/pick-date
// ─────────────────────────────────────────────
pub async fn pick_date(
    auth: AuthUser,
    query: Query<StreamQuery>,
    Json(req): Json<PickDateRequest>,
) -> Response {
    let state = models::get_state();
    let user_id = auth.user_id;
    let is_stream = query.stream.unwrap_or(false);

    // Validate dates
    if chrono::NaiveDate::parse_from_str(&req.start_date, "%Y-%m-%d").is_err() {
        return api_error(
            StatusCode::BAD_REQUEST,
            "start_date must be YYYY-MM-DD format",
        );
    }
    if chrono::NaiveDate::parse_from_str(&req.end_date, "%Y-%m-%d").is_err() {
        return api_error(
            StatusCode::BAD_REQUEST,
            "end_date must be YYYY-MM-DD format",
        );
    }
    if req.activity.trim().is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "activity must not be empty");
    }

    let user_profile = repos::get_user_profile(&state.db_pool, user_id).await;
    let bazi_four_pillars = user_profile
        .bazi_four_pillars
        .as_deref()
        .and_then(|raw| serde_json::from_str::<services::paipan::StructuredBazi>(raw).ok())
        .map(|b| b.to_string());

    let Some(bazi_four_pillars) = bazi_four_pillars.as_deref() else {
        return api_error(
            StatusCode::BAD_REQUEST,
            "No Bazi profile found. Create one first via POST /api/v1/profile.",
        );
    };

    let bazi_summary = user_profile
        .bazi_summary
        .as_deref()
        .unwrap_or_else(|| user_profile.bazi_analysis.as_deref().unwrap_or_default());

    match services::almanac::analysis_pick_selection(services::almanac::PickSelectionRequest {
        start_date: &req.start_date,
        end_date: &req.end_date,
        activity: &req.activity,
        bazi_four_pillars,
        bazi_summary,
        stream: is_stream,
        llm_model: user_profile.llm_model,
        user_id: Some(user_id as i64),
        request_type: Some("api_pick_date".to_string()),
    })
    .await
    {
        Ok(models::LlmResponse::Stream(receiver)) if is_stream => {
            Sse::new(stream_to_sse(receiver)).into_response()
        }
        Ok(models::LlmResponse::Full(analysis)) => {
            Json(ApiResponse::ok(PickData { analysis })).into_response()
        }
        Ok(_) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unexpected response type from LLM",
        ),
        Err(e) => {
            error!("API: Pick date error: {}", e);
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to generate date selection: {}", e),
            )
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
            return api_error(
                StatusCode::BAD_REQUEST,
                format!("Invalid model ID: {}. Valid: 0, 1, 2", req.model),
            );
        }
    };

    repos::update_user_llm_model(&state.db_pool, user_id, req.model).await;

    Json(ApiResponse::ok(ModelData {
        model: model.as_str().to_string(),
    }))
    .into_response()
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

    if let Err(e) =
        repos::update_user_schedule(&state.db_pool, user_id, schedule_val.as_deref()).await
    {
        error!("API: Failed to update schedule: {}", e);
        return api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to update schedule",
        );
    }

    // Note: Schedule runtime updates require the bot instance, which is not available here.
    // The scheduler picks up changes on restart or via the next cron cycle.
    // For live updates, users should use the Telegram /schedule command.

    Json(ApiResponse::ok(ScheduleData { schedule: req.time })).into_response()
}

// ─────────────────────────────────────────────
// POST /api/v1/chat
// ─────────────────────────────────────────────
pub async fn chat(
    auth: AuthUser,
    query: Query<StreamQuery>,
    Json(req): Json<ChatRequest>,
) -> Response {
    let state = models::get_state();
    let user_id = auth.user_id;
    let is_stream = query.stream.unwrap_or(false);

    if req.message.trim().is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "message must not be empty");
    }

    let user_profile = repos::get_user_profile(&state.db_pool, user_id).await;
    if user_profile.bazi_four_pillars.is_none() {
        return api_error(
            StatusCode::BAD_REQUEST,
            "No Bazi profile found. Create one first via POST /api/v1/profile.",
        );
    }

    // Push user message to context
    {
        let mut ctx = state.user_contexts.entry(user_id).or_default();
        ctx.push_message(
            format!("User: {}", req.message),
            state.config.max_context_messages,
        );
        ctx.last_active = chrono::Utc::now();
    }

    let system_prompt_text = include_str!("../../prompts/FollowUpAssistant.md");
    let system_msg =
        match async_openai::types::chat::ChatCompletionRequestSystemMessageArgs::default()
            .content(system_prompt_text)
            .build()
        {
            Ok(m) => m,
            Err(e) => {
                error!("API: Failed to build system message: {}", e);
                return api_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to build prompt");
            }
        };

    let mut messages: Vec<async_openai::types::chat::ChatCompletionRequestMessage> =
        vec![system_msg.into()];

    // Build conversation history from context
    {
        if let Some(ctx) = state.user_contexts.get(&user_id) {
            for m in &ctx.messages {
                if let Some(stripped) = m.strip_prefix("User: ") {
                    if let Ok(msg) =
                        async_openai::types::chat::ChatCompletionRequestUserMessageArgs::default()
                            .content(stripped)
                            .build()
                    {
                        messages.push(msg.into());
                    }
                } else if let Some(stripped) = m.strip_prefix("Assistant: ") {
                    if let Ok(msg) = async_openai::types::chat::ChatCompletionRequestAssistantMessageArgs::default()
                        .content(stripped)
                        .build()
                    {
                        messages.push(msg.into());
                    }
                } else if let Ok(msg) =
                    async_openai::types::chat::ChatCompletionRequestUserMessageArgs::default()
                        .content(m.as_str())
                        .build()
                {
                    messages.push(msg.into());
                }
            }
        }
    }

    let model_name = user_profile
        .llm_model
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| state.config.llm_model_name.clone());

    let mut params = services::llm::LlmRequestParams::new(model_name, messages);
    params.stream = Some(is_stream);
    params.temperature = Some(0.4);
    params.user_id = Some(user_id as i64);
    params.request_type = Some("api_chat".to_string());

    match services::llm::call_llm(&state.db_pool, &state.config.llm_client_config, params).await {
        Ok(models::LlmResponse::Stream(mut receiver)) if is_stream => {
            let (tx, rx) = tokio::sync::mpsc::channel::<String>(100);
            let state_clone = state.clone();

            tokio::spawn(async move {
                let mut full_reply = String::new();
                while let Some(chunk) = receiver.recv().await {
                    full_reply.push_str(&chunk);
                    if tx.send(chunk).await.is_err() {
                        break;
                    }
                }

                // Save assistant response to context after stream completes
                let mut ctx = state_clone.user_contexts.entry(user_id).or_default();
                ctx.push_message(
                    format!("Assistant: {}", full_reply),
                    state_clone.config.max_context_messages,
                );
            });

            Sse::new(stream_to_sse(rx)).into_response()
        }
        Ok(models::LlmResponse::Full(reply)) => {
            // Save assistant response to context
            {
                let mut ctx = state.user_contexts.entry(user_id).or_default();
                ctx.push_message(
                    format!("Assistant: {}", reply),
                    state.config.max_context_messages,
                );
            }
            Json(ApiResponse::ok(ChatData { reply })).into_response()
        }
        Ok(models::LlmResponse::Stream(receiver)) => {
            // Streaming returned but not requested — collect it
            let reply = collect_stream(receiver).await;
            {
                let mut ctx = state.user_contexts.entry(user_id).or_default();
                ctx.push_message(
                    format!("Assistant: {}", reply),
                    state.config.max_context_messages,
                );
            }
            Json(ApiResponse::ok(ChatData { reply })).into_response()
        }
        Err(e) => {
            error!("API: Chat error: {}", e);
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to generate response: {}", e),
            )
        }
    }
}
