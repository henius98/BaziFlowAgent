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
  let user = sqlx::query_scalar::<_, i64>("SELECT user_id FROM users WHERE chart_token = ?1").bind(token).fetch_optional(&state.db_pool).await;
  let Ok(Some(user)) = user else {
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
