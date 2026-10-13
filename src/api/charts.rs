use axum::{
  body::Bytes,
  extract::Path,
  http::{StatusCode, header},
  response::{IntoResponse, Response},
};

/// Capability links replace predictable user-ID URLs. Tokens never enter tracing spans.
pub async fn chart(Path(token): Path<String>) -> Response {
  let state = crate::models::get_state();
  let Ok(_permit) = state.runtime.requests.clone().try_acquire_owned() else {
    return StatusCode::TOO_MANY_REQUESTS.into_response();
  };

  if token == "logo_ai_bot.png" {
    static LOGO: tokio::sync::OnceCell<Bytes> = tokio::sync::OnceCell::const_new();
    let Ok(bytes) = LOGO.get_or_try_init(|| async { tokio::fs::read("public/logo_ai_bot.png").await.map(Bytes::from) }).await else {
      return StatusCode::NOT_FOUND.into_response();
    };
    return ([(header::CONTENT_TYPE, "image/png"), (header::CACHE_CONTROL, "public, max-age=86400, immutable")], bytes.clone()).into_response();
  }

  // Stateless AES-256-GCM verification skips database lookup.
  let Some(user) = crate::services::chart_token::verify_chart_token(&token, &state.config.chart_token_secret) else {
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
      (header::CONTENT_SECURITY_POLICY, "default-src 'none'; img-src 'self'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; base-uri 'none'; frame-ancestors 'none'"),
    ],
    html,
  )
    .into_response()
}
