use axum::{
  Json,
  extract::FromRequestParts,
  http::{StatusCode, request::Parts},
  response::{IntoResponse, Response},
};

use crate::repos;

/// Authenticated API user extracted from the Bearer token.
/// Use as a handler parameter to require authentication.
#[derive(Clone)]
pub struct AuthUser {
  pub user_id: u64,
  key_hash: String,
}

/// Error returned when authentication fails.
pub enum AuthError {
  MissingToken,
  InvalidToken,
}

impl IntoResponse for AuthError {
  fn into_response(self) -> Response {
    let (status, message) = match self {
      AuthError::MissingToken => (StatusCode::UNAUTHORIZED, "Missing Authorization header. Expected: Bearer <api_key>"),
      AuthError::InvalidToken => (StatusCode::UNAUTHORIZED, "Invalid or revoked API key"),
    };
    (status, Json(super::models::ApiError::new(message))).into_response()
  }
}

impl<S: Send + Sync> FromRequestParts<S> for AuthUser {
  type Rejection = AuthError;

  async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
    if let Some(auth) = parts.extensions.get::<AuthUser>() {
      return Ok(auth.clone());
    }
    let auth_header = parts.headers.get("authorization").and_then(|v| v.to_str().ok()).ok_or(AuthError::MissingToken)?;

    let token = auth_header.strip_prefix("Bearer ").ok_or(AuthError::MissingToken)?;

    if token.len() != 36 || !token.starts_with("bfa_") || !token[4..].bytes().all(|b| b.is_ascii_hexdigit()) {
      return Err(AuthError::MissingToken);
    }

    let state = crate::models::get_state();
    let user_id = repos::get_user_id_by_api_key(&state.db_pool, token).await.ok_or(AuthError::InvalidToken)?;

    use sha2::{Digest, Sha256};
    Ok(AuthUser { user_id, key_hash: hex::encode(Sha256::digest(token.as_bytes())) })
  }
}

impl AuthUser {
  pub async fn is_current(&self, state: &crate::models::AppState) -> bool {
    matches!(tokio::time::timeout(std::time::Duration::from_secs(5), repos::is_api_key_current(&state.db_pool, &self.key_hash, self.user_id)).await, Ok(true))
  }
}
