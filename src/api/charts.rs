use axum::{
  body::Bytes,
  extract::Path,
  http::{StatusCode, header},
  response::{IntoResponse, Response},
};

/// Serve compile-time embedded logo image.
pub fn logo() -> Response {
  static LOGO: &[u8] = include_bytes!("../../public/logo_ai_bot.png");
  ([(header::CONTENT_TYPE, "image/png"), (header::CACHE_CONTROL, "public, max-age=86400, immutable")], Bytes::from_static(LOGO)).into_response()
}

/// Capability links replace predictable user-ID URLs. Tokens never enter tracing spans.
pub async fn chart(axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::models::AppState>>, Path(token): Path<String>) -> Response {
  let Ok(_permit) = state.runtime.requests.clone().try_acquire_owned() else {
    return StatusCode::TOO_MANY_REQUESTS.into_response();
  };

  if token == "logo_ai_bot.png" {
    return logo();
  }

  // Stateless AES-256-GCM verification skips database lookup.
  let Some(user) = crate::services::chart_token::verify_chart_token(&token, &state.config.chart_token_secret) else {
    return StatusCode::NOT_FOUND.into_response();
  };
  let Ok(html) = tokio::fs::read(format!("public/bazi_{user}.html")).await else {
    return StatusCode::NOT_FOUND.into_response();
  };
  (
    [(header::CONTENT_TYPE, "text/html; charset=utf-8"), (header::CACHE_CONTROL, "no-store"), (header::REFERRER_POLICY, "no-referrer"), (header::CONTENT_SECURITY_POLICY, baziflow_core::CHART_CSP)],
    html,
  )
    .into_response()
}
