use axum::{
  extract::Path,
  http::{StatusCode, header},
  response::{IntoResponse, Response},
};

/// Capability links replace predictable user-ID URLs. Tokens never enter tracing spans.
pub async fn chart(Path(token): Path<String>) -> Response {
  if token.len() != 64 || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
    return StatusCode::NOT_FOUND.into_response();
  }
  let state = crate::models::get_state();
  let Ok(_permit) = state.runtime.requests.clone().try_acquire_owned() else {
    return StatusCode::TOO_MANY_REQUESTS.into_response();
  };
  let Some(user) = crate::repos::get_user_id_by_chart_token(&state.db_pool, &token).await else {
    return StatusCode::NOT_FOUND.into_response();
  };
  let Ok(html) = tokio::fs::read(format!("public/bazi_{user}.html")).await else {
    return StatusCode::NOT_FOUND.into_response();
  };
  (
    [
      (header::CONTENT_TYPE, "text/html; charset=utf-8"),
      (header::CACHE_CONTROL, "no-store"),
      (header::REFERRER_POLICY, "no-referrer"),
      (
        header::CONTENT_SECURITY_POLICY,
        "default-src 'none'; style-src 'unsafe-inline' https://fonts.googleapis.com; font-src https://fonts.gstatic.com; script-src 'unsafe-inline'; base-uri 'none'; frame-ancestors 'none'",
      ),
    ],
    html,
  )
    .into_response()
}
