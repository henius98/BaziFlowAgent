use axum::{
    Json,
    extract::FromRequestParts,
    http::{StatusCode, request::Parts},
    response::{IntoResponse, Response},
};

use crate::repos;

/// Authenticated API user extracted from the Bearer token.
/// Use as a handler parameter to require authentication.
pub struct AuthUser {
    pub user_id: u64,
}

/// Error returned when authentication fails.
pub enum AuthError {
    MissingToken,
    InvalidToken,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AuthError::MissingToken => (
                StatusCode::UNAUTHORIZED,
                "Missing Authorization header. Expected: Bearer <api_key>",
            ),
            AuthError::InvalidToken => (StatusCode::UNAUTHORIZED, "Invalid or revoked API key"),
        };
        (status, Json(super::models::ApiError::new(message))).into_response()
    }
}

impl<S: Send + Sync> FromRequestParts<S> for AuthUser {
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let auth_header = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or(AuthError::MissingToken)?;

        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or(AuthError::MissingToken)?;

        if token.is_empty() {
            return Err(AuthError::MissingToken);
        }

        let state = crate::models::get_state();
        let user_id = repos::get_user_id_by_api_key(&state.db_pool, token)
            .await
            .ok_or(AuthError::InvalidToken)?;

        Ok(AuthUser { user_id })
    }
}
