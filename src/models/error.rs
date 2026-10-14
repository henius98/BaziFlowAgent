use thiserror::Error;

/// Pure domain errors decoupled from transport, networking, and storage primitives.
pub use baziflow_core::error::DomainError;

/// Boundary errors originating from durable databases or object storage.
#[derive(Error, Debug)]
pub enum StorageError {
  #[error("Database Error: {0}")]
  Db(#[from] sqlx::Error),

  #[error("D1 Database Error: {0}")]
  D1(String),

  #[error("S3 Error: {0}")]
  S3(#[from] s3::error::S3Error),

  #[error("Credentials Error: {0}")]
  Credentials(#[from] s3::creds::error::CredentialsError),

  #[error("IO Error: {0}")]
  Io(#[from] std::io::Error),
}

#[derive(Error, Debug)]
pub enum AppError {
  #[error("Domain Error: {0}")]
  Domain(DomainError),

  #[error("Storage Error: {0}")]
  Storage(#[from] StorageError),

  #[error("HTTP Error: {0}")]
  Http(#[from] reqwest::Error),

  #[error("JSON Error: {0}")]
  Json(#[from] serde_json::Error),

  #[error("Database Error: {0}")]
  Db(#[from] sqlx::Error),

  #[error("D1 Database Error: {0}")]
  D1(String),

  #[error("OpenAI API Error: {0}")]
  OpenAI(#[from] async_openai::error::OpenAIError),

  #[error("System Error: {0}")]
  System(#[from] anyhow::Error),

  #[error("Telegram Error: {0}")]
  Telegram(#[from] teloxide::RequestError),

  #[error("Parse Error: {0}")]
  Parse(#[from] chrono::ParseError),

  #[error("IO Error: {0}")]
  Io(#[from] std::io::Error),

  #[error("S3 Error: {0}")]
  S3(#[from] s3::error::S3Error),

  #[error("Credentials Error: {0}")]
  Credentials(#[from] s3::creds::error::CredentialsError),

  #[error("Profile not found")]
  ProfileNotFound,

  #[error("Application Error: {0}")]
  Message(String),
}

impl From<&str> for AppError {
  fn from(msg: &str) -> Self {
    AppError::Message(msg.to_string())
  }
}

impl From<DomainError> for AppError {
  fn from(err: DomainError) -> Self {
    match err {
      DomainError::ProfileNotFound => AppError::ProfileNotFound,
      DomainError::InvalidInput(m) => AppError::Message(m),
      DomainError::Astrology(m) => AppError::Message(m),
      DomainError::Validation(m) => AppError::Message(m),
      DomainError::Message(m) => AppError::Message(m),
    }
  }
}

pub type AppResult<T> = Result<T, AppError>;

/// Helper extension trait to log and map specific Results quickly
pub trait LogErrorExt<T> {
  fn log_err_msg(self, context_msg: &str) -> AppResult<T>;
}

impl<T, E> LogErrorExt<T> for Result<T, E>
where
  E: Into<AppError>,
{
  fn log_err_msg(self, context_msg: &str) -> AppResult<T> {
    self.map_err(|e| {
      let app_err: AppError = e.into();
      tracing::error!("{}: {}", context_msg, app_err);
      app_err
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_log_error_ext() {
    let err_res: Result<(), &'static str> = Err("test error");
    let mapped = err_res.log_err_msg("Test context");
    assert!(mapped.is_err());
    match mapped.unwrap_err() {
      AppError::Message(m) => assert_eq!(m, "test error"),
      _ => panic!("unexpected error variant"),
    }

    let ok_res: Result<i32, &'static str> = Ok(42);
    assert_eq!(ok_res.log_err_msg("Should not fail").expect("valid ok value"), 42);
  }
}
