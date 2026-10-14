use thiserror::Error;

/// Pure domain errors decoupled from transport, networking, and storage primitives.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
  #[error("Profile not found")]
  ProfileNotFound,

  #[error("Invalid input: {0}")]
  InvalidInput(String),

  #[error("Astrology calculation error: {0}")]
  Astrology(String),

  #[error("Summary validation error: {0}")]
  Validation(String),

  #[error("Domain error: {0}")]
  Message(String),
}

impl From<&str> for DomainError {
  fn from(msg: &str) -> Self {
    DomainError::Message(msg.to_string())
  }
}
