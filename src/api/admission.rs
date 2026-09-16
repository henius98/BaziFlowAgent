//! Admission applies before authentication I/O and retains permits through streaming bodies.
use super::auth::AuthUser;
use axum::extract::FromRequestParts;
use axum::{
  body::Body,
  extract::Request,
  http::{Method, StatusCode},
  middleware::Next,
  response::{IntoResponse, Response},
};
use futures::StreamExt;
use std::time::Duration;

pub async fn admit(request: Request, next: Next) -> Response {
  let state = crate::models::get_state();
  if state.runtime.shutdown.is_cancelled() {
    return StatusCode::SERVICE_UNAVAILABLE.into_response();
  }
  let Ok(permit) = state.runtime.requests.clone().try_acquire_owned() else {
    return StatusCode::TOO_MANY_REQUESTS.into_response();
  };
  let (mut parts, body) = request.into_parts();
  let auth = match tokio::time::timeout(Duration::from_secs(5), AuthUser::from_request_parts(&mut parts, &())).await {
    Ok(Ok(auth)) => auth,
    Ok(Err(error)) => return error.into_response(),
    Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
  };
  if !state.runtime.allow(auth.user_id, state.config.runtime.requests_per_minute) {
    return StatusCode::TOO_MANY_REQUESTS.into_response();
  }
  let processing = if parts.method == Method::POST || parts.method == Method::PUT {
    match crate::models::ProcessingGuard::acquire(state.clone(), auth.user_id) {
      Some(guard) => Some(guard),
      None => return StatusCode::TOO_MANY_REQUESTS.into_response(),
    }
  } else {
    None
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
    let _processing = processing;
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
