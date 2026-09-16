mod admission;
pub mod auth;
pub mod charts;
pub mod gateway;
pub mod handlers;
pub mod models;

use axum::Router;
use axum::routing::{get, post, put};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use std::sync::Arc;

/// Build the API router with all v1 endpoints.
/// Auth middleware is applied per-handler via the `AuthUser` extractor.
pub fn api_router(config: Arc<crate::config::AppConfig>) -> Router {
  let cors_layer = if config.cors_allowed_origin == "*" {
    CorsLayer::permissive()
  } else {
    let origins = config
      .cors_allowed_origin
      .split(',')
      .map(|s| {
        let trimmed = s.trim();
        let without_slash = trimmed.strip_suffix('/').unwrap_or(trimmed);
        without_slash.parse::<axum::http::HeaderValue>().expect("Invalid CORS_ALLOWED_ORIGIN")
      })
      .collect::<Vec<_>>();
    CorsLayer::new().allow_origin(origins).allow_methods(tower_http::cors::Any).allow_headers(tower_http::cors::Any)
  };

  Router::new()
    .route("/api/v1/ws", get(gateway::upgrade))
    .route("/api/v1/profile", get(handlers::get_profile).post(handlers::create_profile))
    .route("/api/v1/date-fortune", get(handlers::date_fortune))
    .route("/api/v1/pick-date", post(handlers::pick_date))
    .route("/api/v1/model", put(handlers::update_model))
    .route("/api/v1/schedule", put(handlers::update_schedule))
    .route("/api/v1/chat", post(handlers::chat))
    .layer(axum::extract::DefaultBodyLimit::max(config.runtime.max_message_bytes))
    .layer(axum::middleware::from_fn(admission::admit))
    .layer(cors_layer)
    .layer(TraceLayer::new_for_http().make_span_with(|request: &axum::http::Request<axum::body::Body>| tracing::info_span!("http_request", method = %request.method(), path = request.uri().path())))
}

/// Low-latency sockets for request/response and incremental streaming workloads.
pub fn listener(listener: tokio::net::TcpListener) -> impl axum::serve::Listener<Io = tokio::net::TcpStream, Addr = std::net::SocketAddr> {
  use axum::serve::ListenerExt;
  listener.tap_io(|stream| {
    if let Err(error) = stream.set_nodelay(true) {
      tracing::warn!(%error, "Failed to enable TCP_NODELAY");
    }
  })
}
