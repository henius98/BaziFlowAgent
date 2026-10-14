//! Admission applies before authentication I/O and retains permits through streaming bodies.
use super::auth::AuthUser;
use crate::models::runtime::AdmissionRejection;
use axum::extract::FromRequestParts;
use axum::{
  body::Body,
  extract::Request,
  http::{Method, StatusCode, header},
  middleware::Next,
  response::{IntoResponse, Response},
};
use futures::StreamExt;
use std::time::Duration;

pub async fn admit(axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::models::AppState>>, request: Request, next: Next) -> Response {
  if state.runtime.shutdown.is_cancelled() {
    return StatusCode::SERVICE_UNAVAILABLE.into_response();
  }
  let (mut parts, body) = request.into_parts();
  let auth = match tokio::time::timeout(Duration::from_secs(5), AuthUser::from_request_parts(&mut parts, &state)).await {
    Ok(Ok(auth)) => auth,
    Ok(Err(error)) => return error.into_response(),
    Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
  };

  let require_exclusive = parts.method == Method::POST || parts.method == Method::PUT;
  let permit = match state.runtime.admission.try_admit(auth.user_id, auth.rate_limit, require_exclusive) {
    Ok(permit) => permit,
    Err(AdmissionRejection::ServerShuttingDown) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    Err(AdmissionRejection::MaxRequestsExceeded) => {
      return (StatusCode::TOO_MANY_REQUESTS, [(header::RETRY_AFTER, "5")]).into_response();
    }
    Err(AdmissionRejection::RateLimitExceeded) => {
      return (StatusCode::TOO_MANY_REQUESTS, [(header::RETRY_AFTER, crate::models::runtime::AdmissionController::RATE_LIMIT_RETRY_AFTER)]).into_response();
    }
    Err(AdmissionRejection::UserAlreadyProcessing) => {
      return (StatusCode::TOO_MANY_REQUESTS, [(header::RETRY_AFTER, "2")]).into_response();
    }
  };

  parts.extensions.insert(auth);
  let response = tokio::select! {
    _ = state.runtime.shutdown.cancelled() => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    result = tokio::time::timeout(Duration::from_secs(state.config.runtime.work_seconds), next.run(Request::from_parts(parts, body))) => {
      match result { Ok(response) => response, Err(_) => return StatusCode::GATEWAY_TIMEOUT.into_response() }
    }
  };
  if response.headers().get("content-type").is_none_or(|value| value != "text/event-stream") {
    return response;
  }
  let (parts, body) = response.into_parts();
  let stream = async_stream::stream! {
    let _permit = permit;
    let mut body = body.into_data_stream();
    let deadline = tokio::time::sleep(Duration::from_secs(state.config.runtime.work_seconds));
    tokio::pin!(deadline);
    loop {
      tokio::select! {
        _ = state.runtime.shutdown.cancelled() => break,
        _ = &mut deadline => break,
        chunk = body.next() => match chunk { Some(chunk) => yield chunk, None => break }
      }
    }
  };
  Response::from_parts(parts, Body::from_stream(stream))
}
