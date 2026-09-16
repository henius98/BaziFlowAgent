//! A best-effort, disposable cache for recent chat turns.
//!
//! This intentionally lives in its own SQLite database so cache-heavy writes do not
//! contend with the durable user/profile database.

use sqlx::{
  SqlitePool,
  sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use std::{collections::HashSet, str::FromStr};
use tokio::sync::mpsc;
use tracing::error;

const CREATE_CHAT_HISTORY_TABLE: &str = r#"
    CREATE TABLE IF NOT EXISTS chat_history (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        user_id INTEGER NOT NULL,
        role TEXT NOT NULL CHECK(role IN ('user', 'assistant')),
        content TEXT NOT NULL,
        created_at INTEGER NOT NULL DEFAULT (unixepoch())
    )
"#;

const WRITE_QUEUE_CAPACITY: usize = 128;
const MAX_WRITE_BATCH_SIZE: usize = 32;

/// Cache database pool and its non-blocking background writer.
pub struct ChatHistoryCache {
  pub pool: SqlitePool,
  pub writer: ChatHistoryWriter,
}

#[derive(Clone, Debug)]
pub struct ChatHistoryWriter {
  sender: mpsc::Sender<ChatHistoryOperation>,
}

#[derive(Debug, thiserror::Error)]
pub enum ChatHistoryQueueError {
  #[error("chat history write queue is full")]
  Full,
  #[error("chat history message exceeds 64 KiB")]
  TooLarge,
  #[error("chat history writer has stopped")]
  Closed,
}

#[derive(Debug)]
enum ChatHistoryOperation {
  Save { user_id: u64, role: String, content: String },
  Clear { user_id: u64 },
  Flush(tokio::sync::oneshot::Sender<()>),
}

impl ChatHistoryWriter {
  pub async fn flush(&self) -> Result<(), ChatHistoryQueueError> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    self.sender.send(ChatHistoryOperation::Flush(sender)).await.map_err(|_| ChatHistoryQueueError::Closed)?;
    receiver.await.map_err(|_| ChatHistoryQueueError::Closed)
  }

  /// Queue a write without making the request wait for SQLite I/O.
  pub fn save(&self, user_id: u64, role: impl Into<String>, content: impl Into<String>) -> Result<(), ChatHistoryQueueError> {
    let content = content.into();
    if content.len() > 65_536 {
      return Err(ChatHistoryQueueError::TooLarge);
    }
    self.enqueue(ChatHistoryOperation::Save { user_id, role: role.into(), content })
  }

  /// Queue removal of a user's history before starting a standalone reading.
  pub async fn clear(&self, user_id: u64) -> Result<(), ChatHistoryQueueError> {
    self.sender.send(ChatHistoryOperation::Clear { user_id }).await.map_err(|_| ChatHistoryQueueError::Closed)?;
    self.flush().await
  }

  fn enqueue(&self, operation: ChatHistoryOperation) -> Result<(), ChatHistoryQueueError> {
    match self.sender.try_send(operation) {
      Ok(()) => Ok(()),
      Err(mpsc::error::TrySendError::Full(_)) => Err(ChatHistoryQueueError::Full),
      Err(mpsc::error::TrySendError::Closed(_)) => Err(ChatHistoryQueueError::Closed),
    }
  }
}

/// Open the dedicated chat-history cache with write-optimized SQLite settings.
pub async fn init_chat_history_cache(db_url: &str, max_messages_per_user: usize) -> Result<ChatHistoryCache, sqlx::Error> {
  let options = SqliteConnectOptions::from_str(db_url)?
    .create_if_missing(true)
    // WAL permits readers to continue while a writer is active.
    .pragma("journal_mode", "WAL")
    // Cache data is disposable, so do not wait for physical disk persistence.
    .pragma("synchronous", "OFF")
    .pragma("temp_store", "MEMORY")
    .pragma("cache_size", "-8192")
    .pragma("mmap_size", "268435456")
    // Retry briefly rather than failing when another WAL write is in progress.
    .pragma("busy_timeout", "3000");

  let pool = SqlitePoolOptions::new()
    // SQLite has one writer; a small pool avoids needless connection contention.
    .max_connections(4)
    .connect_with(options)
    .await?;

  sqlx::query(CREATE_CHAT_HISTORY_TABLE).execute(&pool).await?;
  sqlx::query("CREATE INDEX IF NOT EXISTS idx_chat_history_user_id_id ON chat_history (user_id, id)").execute(&pool).await?;

  let writer = start_chat_history_writer(pool.clone(), max_messages_per_user);
  Ok(ChatHistoryCache { pool, writer })
}

/// Start a write-behind worker. Writes are committed in small transactions.
pub fn start_chat_history_writer(pool: SqlitePool, max_messages_per_user: usize) -> ChatHistoryWriter {
  let (sender, mut receiver) = mpsc::channel(WRITE_QUEUE_CAPACITY);
  tokio::spawn(async move {
    while let Some(first) = receiver.recv().await {
      let mut operations = vec![first];
      while operations.len() < MAX_WRITE_BATCH_SIZE {
        match receiver.try_recv() {
          Ok(operation) => operations.push(operation),
          Err(mpsc::error::TryRecvError::Empty | mpsc::error::TryRecvError::Disconnected) => {
            break;
          }
        }
      }

      if let Err(e) = write_batch(&pool, operations, max_messages_per_user).await {
        error!("Failed to write chat history cache batch: {}", e);
      }
    }
  });

  ChatHistoryWriter { sender }
}

async fn write_batch(pool: &SqlitePool, operations: Vec<ChatHistoryOperation>, max_messages_per_user: usize) -> Result<(), sqlx::Error> {
  let mut transaction = pool.begin().await?;
  let mut touched_users = HashSet::new();
  let mut barriers = Vec::new();

  for operation in operations {
    match operation {
      ChatHistoryOperation::Save { user_id, role, content } => {
        sqlx::query("INSERT INTO chat_history (user_id, role, content) VALUES (?1, ?2, ?3)").bind(user_id as i64).bind(role).bind(content).execute(&mut *transaction).await?;
        touched_users.insert(user_id);
      }
      ChatHistoryOperation::Flush(sender) => barriers.push(sender),
      ChatHistoryOperation::Clear { user_id } => {
        sqlx::query("DELETE FROM chat_history WHERE user_id = ?1").bind(user_id as i64).execute(&mut *transaction).await?;
        touched_users.insert(user_id);
      }
    }
  }

  for user_id in touched_users {
    trim_chat_history(&mut transaction, user_id, max_messages_per_user).await?;
  }

  transaction.commit().await?;
  for barrier in barriers {
    let _ = barrier.send(());
  }
  Ok(())
}

async fn trim_chat_history(transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>, user_id: u64, max_messages_per_user: usize) -> Result<(), sqlx::Error> {
  sqlx::query(
    r#"
        DELETE FROM chat_history
        WHERE user_id = ?1
          AND id NOT IN (
              SELECT id FROM chat_history
              WHERE user_id = ?1
              ORDER BY id DESC
              LIMIT ?2
          )
        "#,
  )
  .bind(user_id as i64)
  .bind(max_messages_per_user as i64)
  .execute(&mut **transaction)
  .await?;

  Ok(())
}

/// Return the latest turns in chronological order, formatted for `build_chat_messages`.
pub async fn recent_chat_messages(pool: &SqlitePool, user_id: u64, limit: usize) -> Result<Vec<String>, sqlx::Error> {
  if limit == 0 {
    return Ok(Vec::new());
  }

  let rows = sqlx::query_as::<_, (String, String)>(
    r#"
        SELECT role, content
        FROM (
            SELECT role, content, id
            FROM chat_history
            WHERE user_id = ?1
            ORDER BY id DESC
            LIMIT ?2
        )
        ORDER BY id ASC
        "#,
  )
  .bind(user_id as i64)
  .bind(limit as i64)
  .fetch_all(pool)
  .await?;

  Ok(
    rows
      .into_iter()
      .map(|(role, content)| match role.as_str() {
        "assistant" => format!("Assistant: {content}"),
        _ => format!("User: {content}"),
      })
      .collect(),
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn saves_reads_and_clears_user_history() {
    let path = std::env::temp_dir().join(format!("baziflow-chat-cache-{}.db", uuid::Uuid::now_v7()));
    let db_url = format!("sqlite://{}", path.display());
    let cache = init_chat_history_cache(&db_url, 2).await.unwrap();
    let pool = cache.pool;

    cache.writer.save(42, "user", "First question").unwrap();
    cache.writer.save(42, "assistant", "First answer").unwrap();
    cache.writer.save(42, "user", "Second question").unwrap();

    tokio::time::timeout(std::time::Duration::from_secs(1), async {
      loop {
        if recent_chat_messages(&pool, 42, 10).await.unwrap().len() == 2 {
          break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
      }
    })
    .await
    .unwrap();

    assert_eq!(recent_chat_messages(&pool, 42, 10).await.unwrap(), vec!["Assistant: First answer".to_string(), "User: Second question".to_string(),]);

    cache.writer.clear(42).await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
      loop {
        if recent_chat_messages(&pool, 42, 10).await.unwrap().is_empty() {
          break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
      }
    })
    .await
    .unwrap();
    assert!(recent_chat_messages(&pool, 42, 10).await.unwrap().is_empty());

    pool.close().await;
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
  }
}
