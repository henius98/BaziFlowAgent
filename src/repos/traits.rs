//! Repository traits and abstraction contracts for BaziFlowAgent.
//! Supports dynamic dispatch (`Arc<dyn ...>`) via `futures::future::BoxFuture`.

use crate::models::error::AppResult;
use futures::future::BoxFuture;

/// Relational data access for users, profiles, schedules, and API keys.
pub use crate::repos::Database as UserRepository;

/// Telemetry and audit logging for LLM requests and responses.
pub use crate::repos::Database as LlmLogRepository;

pub use crate::models::common::ChatRole;
pub use crate::models::common::ChatRole as CacheRole;

/// Structured chat turn record for zero-copy cache representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatTurnRecord {
  pub role: ChatRole,
  pub content: String,
}

/// Write-behind cache error variants.
pub use crate::repos::chat_history_cache::ChatHistoryQueueError as ChatCacheQueueError;

/// Disposable, write-behind cache repository for recent chat messages.
pub use crate::repos::chat_history_cache::ChatHistoryCache as ChatCacheRepository;

/// Abstract object / blob storage for HTML diagrams and presigned URLs.
pub trait BlobStore: Send + Sync + std::fmt::Debug {
  fn put<'a>(&'a self, key: &'a str, data: &'a [u8], content_type: Option<&'a str>) -> BoxFuture<'a, AppResult<()>>;

  fn get<'a>(&'a self, key: &'a str) -> BoxFuture<'a, AppResult<Vec<u8>>>;
  fn exists<'a>(&'a self, key: &'a str) -> BoxFuture<'a, AppResult<bool>>;
  fn presign_get<'a>(&'a self, key: &'a str, expiry_secs: u64) -> BoxFuture<'a, AppResult<String>>;
  fn delete<'a>(&'a self, key: &'a str) -> BoxFuture<'a, AppResult<()>>;
}
