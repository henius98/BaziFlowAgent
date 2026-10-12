//! Repository traits and abstraction contracts for BaziFlowAgent.
//! Supports dynamic dispatch (`Arc<dyn ...>`) via `futures::future::BoxFuture`.

use crate::models::common::LlmModel;
use crate::models::error::AppResult;
use crate::repos::{LlmLogParams, UserProfileData, UserSnapshotData};
use futures::future::BoxFuture;

/// Relational data access for users, profiles, schedules, and API keys.
pub trait UserRepository: Send + Sync + std::fmt::Debug {
  fn upsert_user_bazi<'a>(&'a self, user_id: u64, username: Option<&'a str>, bazi_four_pillars: &'a str, gender: u8, birth_datetime: &'a str) -> BoxFuture<'a, AppResult<()>>;

  fn save_user_bazi_analysis<'a>(&'a self, user_id: u64, reading: &'a str) -> BoxFuture<'a, ()>;
  fn save_user_bazi_summary<'a>(&'a self, user_id: u64, summary: &'a str) -> BoxFuture<'a, ()>;
  fn get_user_profile<'a>(&'a self, user_id: u64) -> BoxFuture<'a, UserProfileData>;
  fn get_user_snapshot<'a>(&'a self, user_id: u64) -> BoxFuture<'a, AppResult<UserSnapshotData>>;
  fn get_all_scheduled_users<'a>(&'a self) -> BoxFuture<'a, Vec<(u64, String)>>;
  fn update_user_schedule<'a>(&'a self, user_id: u64, schedule: Option<&'a str>) -> BoxFuture<'a, AppResult<()>>;
  fn get_user_rate_limit<'a>(&'a self, user_id: u64) -> BoxFuture<'a, Option<usize>>;
  fn set_user_rate_limit<'a>(&'a self, user_id: u64, rate_limit: Option<usize>) -> BoxFuture<'a, AppResult<()>>;
  fn get_user_last_active<'a>(&'a self, user_id: u64) -> BoxFuture<'a, AppResult<Option<i64>>>;
  fn get_user_last_active_formatted<'a>(&'a self, user_id: u64) -> BoxFuture<'a, AppResult<Option<String>>>;
  fn touch_user_last_active<'a>(&'a self, user_id: u64) -> BoxFuture<'a, AppResult<()>>;
  fn get_llm_models<'a>(&'a self) -> BoxFuture<'a, AppResult<Vec<LlmModel>>>;
  fn get_default_llm_model<'a>(&'a self) -> BoxFuture<'a, AppResult<LlmModel>> {
    Box::pin(async move { self.get_llm_models().await?.into_iter().next().ok_or_else(|| crate::models::AppError::Message("No enabled LLM models available in database".into())) })
  }
  fn resolve_llm_model<'a>(&'a self, preferred: Option<LlmModel>) -> BoxFuture<'a, AppResult<LlmModel>> {
    Box::pin(async move {
      match preferred {
        Some(m) => Ok(m),
        None => self.get_default_llm_model().await,
      }
    })
  }
  fn update_user_llm_model<'a>(&'a self, user_id: u64, llm_model: u8) -> BoxFuture<'a, AppResult<LlmModel>>;
  fn create_api_key<'a>(&'a self, user_id: u64) -> BoxFuture<'a, AppResult<String>>;
  fn get_auth_user_by_api_key<'a>(&'a self, raw_key: &'a str) -> BoxFuture<'a, Option<(u64, Option<usize>)>>;
  fn get_api_key_info<'a>(&'a self, user_id: u64) -> BoxFuture<'a, Option<(String, String)>>;
  fn get_username_by_user_id<'a>(&'a self, user_id: u64) -> BoxFuture<'a, Option<String>>;
}

/// Telemetry and audit logging for LLM requests and responses.
pub trait LlmLogRepository: Send + Sync + std::fmt::Debug {
  fn save_llm_log<'a>(&'a self, params: LlmLogParams<'a>) -> BoxFuture<'a, ()>;
}

/// Structured chat turn record for zero-copy cache representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheRole {
  User,
  Assistant,
  Tool,
}

impl CacheRole {
  pub fn as_str(&self) -> &'static str {
    match self {
      Self::User => "user",
      Self::Assistant => "assistant",
      Self::Tool => "tool",
    }
  }
}

impl std::str::FromStr for CacheRole {
  type Err = std::convert::Infallible;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    Ok(match s {
      "assistant" => Self::Assistant,
      "tool" => Self::Tool,
      _ => Self::User,
    })
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatTurnRecord {
  pub role: CacheRole,
  pub content: String,
}

/// Write-behind cache error variants.
#[derive(Debug, thiserror::Error)]
pub enum ChatCacheQueueError {
  #[error("chat cache write queue is full")]
  Full,
  #[error("message exceeds capacity limit")]
  TooLarge,
  #[error("chat cache writer is closed")]
  Closed,
}

/// Disposable, write-behind cache repository for recent chat messages.
pub trait ChatCacheRepository: Send + Sync + std::fmt::Debug {
  fn save(&self, user_id: u64, role: CacheRole, content: &str) -> Result<(), ChatCacheQueueError>;
  fn clear<'a>(&'a self, user_id: u64) -> BoxFuture<'a, Result<(), ChatCacheQueueError>>;
  fn flush<'a>(&'a self) -> BoxFuture<'a, Result<(), ChatCacheQueueError>>;
  fn recent_turns<'a>(&'a self, user_id: u64, limit: usize) -> BoxFuture<'a, AppResult<Vec<ChatTurnRecord>>>;
}

/// Abstract object / blob storage for HTML diagrams and presigned URLs.
pub trait BlobStore: Send + Sync + std::fmt::Debug {
  fn put<'a>(&'a self, key: &'a str, data: &'a [u8], content_type: Option<&'a str>) -> BoxFuture<'a, AppResult<()>>;

  fn get<'a>(&'a self, key: &'a str) -> BoxFuture<'a, AppResult<Vec<u8>>>;
  fn exists<'a>(&'a self, key: &'a str) -> BoxFuture<'a, AppResult<bool>>;
  fn presign_get<'a>(&'a self, key: &'a str, expiry_secs: u64) -> BoxFuture<'a, AppResult<String>>;
  fn delete<'a>(&'a self, key: &'a str) -> BoxFuture<'a, AppResult<()>>;
}
