pub mod auth;
pub mod handlers;
pub mod models;

use axum::Router;
use axum::routing::{get, post, put};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

/// Build the API router with all v1 endpoints.
/// Auth middleware is applied per-handler via the `AuthUser` extractor.
pub fn api_router() -> Router {
    Router::new()
        .route(
            "/api/v1/profile",
            get(handlers::get_profile).post(handlers::create_profile),
        )
        .route("/api/v1/date-fortune", post(handlers::date_fortune))
        .route("/api/v1/pick-date", post(handlers::pick_date))
        .route("/api/v1/model", put(handlers::update_model))
        .route("/api/v1/schedule", put(handlers::update_schedule))
        .route("/api/v1/chat", post(handlers::chat))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}
