use crate::models::common::LlmModel;
use sqlx::{
  SqlitePool,
  sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use std::str::FromStr;
use tracing::{error, info};

pub mod chat_history_cache;
mod d1;

pub(crate) static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

#[derive(Debug, Clone)]
pub enum Database {
  Sqlite(SqlitePool),
  D1(d1::D1Database),
}

impl From<SqlitePool> for Database {
  fn from(pool: SqlitePool) -> Self {
    Self::Sqlite(pool)
  }
}

impl Database {
  pub async fn close(&self) {
    if let Self::Sqlite(pool) = self {
      pool.close().await;
    }
  }
}

// Log IDs are allocated atomically by SQLite's INTEGER PRIMARY KEY.
// This replaces timestamp-based random IDs, which collided under bursts.
pub async fn init_db(db_url: &str) -> Result<SqlitePool, sqlx::Error> {
  let options = SqliteConnectOptions::from_str(db_url)?.create_if_missing(true).pragma("journal_mode", "WAL").pragma("synchronous", "NORMAL");

  let pool = SqlitePoolOptions::new().max_connections(20).connect_with(options).await?;

  // Automatically apply any pending migrations.
  // VersionMismatch means migration history and the filesystem disagree.
  // Preserve migration metadata and fail closed so an operator can investigate.
  // Never drop the migrations table to force a resync.
  // Retry only after the migration mismatch has been resolved explicitly.
  MIGRATOR.run(&pool).await?;
  info!("Database migrations applied successfully.");

  Ok(pool)
}

pub async fn init_database(db_url: &str, d1_config: Option<&crate::config::D1Config>, http_client: reqwest::Client) -> crate::models::AppResult<Database> {
  match d1_config {
    Some(config) => {
      let database = d1::D1Database::connect(http_client, config).await?;
      info!("Cloudflare D1 database initialized and migrations applied.");
      Ok(Database::D1(database))
    }
    None => Ok(Database::Sqlite(init_db(db_url).await?)),
  }
}

pub async fn upsert_user_bazi(database: &Database, user_id: u64, username: Option<&str>, bazi_four_pillars: &str, gender: u8, birth_datetime: &str) -> crate::models::AppResult<()> {
  const SQL: &str = r#"
        INSERT INTO users (user_id, username, bazi_four_pillars, gender, birth_datetime)
        VALUES (?1, ?2, jsonb(?3), ?4, ?5)
        ON CONFLICT(user_id) DO UPDATE SET
            username = excluded.username,
            bazi_four_pillars = excluded.bazi_four_pillars,
            gender = excluded.gender,
            birth_datetime = excluded.birth_datetime,
            bazi_analysis = NULL,
            bazi_summary = NULL
        "#;

  match database {
    Database::Sqlite(pool) => {
      sqlx::query(SQL).bind(user_id as i64).bind(username).bind(bazi_four_pillars).bind(gender).bind(birth_datetime).execute(pool).await?;
    }
    Database::D1(database) => {
      // D1 supports SQLite's JSON text functions but not the jsonb() function
      // used by the local database, so adapt only that backend-specific call.
      let d1_sql = SQL.replace("jsonb(?3)", "json(?3)");
      database
        .execute(
          &d1_sql,
          vec![
            serde_json::json!(user_id),
            username.map_or(serde_json::Value::Null, |value| serde_json::json!(value)),
            serde_json::json!(bazi_four_pillars),
            serde_json::json!(gender),
            serde_json::json!(birth_datetime),
          ],
        )
        .await?;
    }
  }

  Ok(())
}

pub async fn save_user_bazi_analysis(database: &Database, user_id: u64, reading: &str) {
  const SQL: &str = r#"
        UPDATE users SET bazi_analysis = ?2
        WHERE user_id = ?1
        "#;
  let result = match database {
    Database::Sqlite(pool) => sqlx::query(SQL).bind(user_id as i64).bind(reading).execute(pool).await.map(|_| ()).map_err(Into::into),
    Database::D1(database) => database.execute(SQL, vec![serde_json::json!(user_id), serde_json::json!(reading)]).await,
  };

  if let Err(e) = result {
    error!("Failed to save bazi_analysis: {}", e);
  }
}

pub struct UserProfileData {
  pub bazi_four_pillars: Option<String>,
  pub bazi_analysis: Option<String>,
  pub bazi_summary: Option<String>,
  pub llm_model: Option<LlmModel>,
  pub schedule: Option<String>,
}

type UserProfileRow = (Option<String>, Option<String>, Option<String>, Option<u8>, Option<String>);

pub async fn get_user_profile(database: &Database, user_id: u64) -> UserProfileData {
  const SQL: &str = r#"SELECT json(bazi_four_pillars) AS bazi_four_pillars, bazi_analysis, bazi_summary, llm_model, schedule FROM users WHERE user_id = ?1"#;
  let row: Option<UserProfileRow> = match database {
    Database::Sqlite(pool) => sqlx::query_as(SQL).bind(user_id as i64).fetch_optional(pool).await.map_err(Into::into),
    Database::D1(database) => database.fetch_all(SQL, vec![serde_json::json!(user_id)]).await.map(|rows| {
      rows.first().map(|row| (json_string(row, "bazi_four_pillars"), json_string(row, "bazi_analysis"), json_string(row, "bazi_summary"), json_u8(row, "llm_model"), json_string(row, "schedule")))
    }),
  }
  .unwrap_or_else(|e: crate::models::AppError| {
    error!("Failed to fetch user profile for {}: {}", user_id, e);
    None
  });

  match row {
    Some(r) => UserProfileData { bazi_four_pillars: r.0, bazi_analysis: r.1, bazi_summary: r.2, llm_model: r.3.and_then(LlmModel::from_u8), schedule: r.4 },
    None => UserProfileData { bazi_four_pillars: None, bazi_analysis: None, bazi_summary: None, llm_model: None, schedule: None },
  }
}

/// Fetch all users who have a non-null schedule for scheduled fortune generation.
pub async fn get_all_scheduled_users(database: &Database) -> Vec<(u64, String)> {
  const SQL: &str = r#"SELECT user_id, schedule FROM users WHERE schedule IS NOT NULL AND schedule != '' AND bazi_four_pillars IS NOT NULL AND bazi_four_pillars != ''"#;
  let rows = match database {
    Database::Sqlite(pool) => sqlx::query_as::<_, (i64, String)>(SQL).fetch_all(pool).await.map_err(Into::into),
    Database::D1(database) => database.fetch_all(SQL, Vec::new()).await.map(|rows| rows.into_iter().filter_map(|row| Some((json_i64(&row, "user_id")?, json_string(&row, "schedule")?))).collect()),
  }
  .unwrap_or_else(|e: crate::models::AppError| {
    error!("Failed to fetch scheduled users: {}", e);
    Vec::new()
  });

  rows.into_iter().map(|(user_id, schedule)| (user_id as u64, schedule)).collect()
}

pub struct LlmLogParams<'a> {
  pub model: &'a str,
  pub user_id: Option<i64>,
  pub request_type: Option<&'a str>,
  pub request_body: &'a str,
  pub response_body: &'a str,
  pub total_tokens: Option<i64>,
  pub duration_ms: i64,
  pub is_success: bool,
}

/// Persist an LLM call log entry (request + response or error).
pub async fn save_llm_log(database: &Database, params: LlmLogParams<'_>) {
  const SQL: &str = r#"
        INSERT INTO llm_logs (model, user_id, request_type, request_body, response_body, total_tokens, duration_ms, is_success)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        "#;
  let result = match database {
    Database::Sqlite(pool) => sqlx::query(SQL)
      .bind(params.model)
      .bind(params.user_id)
      .bind(params.request_type)
      .bind(params.request_body)
      .bind(params.response_body)
      .bind(params.total_tokens)
      .bind(params.duration_ms)
      .bind(params.is_success as i32)
      .execute(pool)
      .await
      .map(|_| ())
      .map_err(Into::into),
    Database::D1(database) => {
      database
        .execute(
          SQL,
          vec![
            serde_json::json!(params.model),
            params.user_id.map_or(serde_json::Value::Null, |value| serde_json::json!(value)),
            params.request_type.map_or(serde_json::Value::Null, |value| serde_json::json!(value)),
            serde_json::json!(params.request_body),
            serde_json::json!(params.response_body),
            params.total_tokens.map_or(serde_json::Value::Null, |value| serde_json::json!(value)),
            serde_json::json!(params.duration_ms),
            serde_json::json!(params.is_success as i32),
          ],
        )
        .await
    }
  };

  if let Err(e) = result {
    error!("Failed to save LLM log: {}", e);
  }
}

pub async fn update_user_llm_model(database: &Database, user_id: u64, llm_model: u8) -> crate::models::AppResult<()> {
  const SQL: &str = r#"
        INSERT INTO users (user_id, llm_model)
        VALUES (?1, ?2)
        ON CONFLICT(user_id) DO UPDATE SET
            llm_model = excluded.llm_model
        "#;
  let result = match database {
    Database::Sqlite(pool) => sqlx::query(SQL).bind(user_id as i64).bind(llm_model).execute(pool).await.map(|_| ()).map_err(Into::into),
    Database::D1(database) => database.execute(SQL, vec![serde_json::json!(user_id), serde_json::json!(llm_model)]).await,
  };

  if let Err(e) = &result {
    error!("Failed to update user LLM model: {}", e);
  }
  result
}

pub async fn save_user_bazi_summary(database: &Database, user_id: u64, summary: &str) {
  const SQL: &str = r#"
        UPDATE users SET bazi_summary = ?2
        WHERE user_id = ?1
        "#;
  let result = match database {
    Database::Sqlite(pool) => sqlx::query(SQL).bind(user_id as i64).bind(summary).execute(pool).await.map(|_| ()).map_err(Into::into),
    Database::D1(database) => database.execute(SQL, vec![serde_json::json!(user_id), serde_json::json!(summary)]).await,
  };

  if let Err(e) = result {
    error!("Failed to save bazi_summary: {}", e);
  }
}

pub async fn update_user_schedule(database: &Database, user_id: u64, schedule: Option<&str>) -> crate::models::error::AppResult<()> {
  const SQL: &str = r#"
        UPDATE users SET schedule = ?2
        WHERE user_id = ?1
        "#;
  let result = match database {
    Database::Sqlite(pool) => sqlx::query(SQL).bind(user_id as i64).bind(schedule).execute(pool).await.map(|_| ()).map_err(Into::into),
    Database::D1(database) => database.execute(SQL, vec![serde_json::json!(user_id), schedule.map_or(serde_json::Value::Null, |value| serde_json::json!(value))]).await,
  };

  if let Err(e) = &result {
    error!("Failed to update user schedule: {}", e);
  }

  result
}

// ─────────────────────────────────────────────
// API Key Management
// ─────────────────────────────────────────────

/// Generate a new API key for the user. Revokes any existing key.
/// Returns the raw key (shown once to user). Only the SHA-256 hash is stored.
pub async fn create_api_key(database: &Database, user_id: u64) -> crate::models::AppResult<String> {
  use rand::Rng;
  use sha2::{Digest, Sha256};

  // Generate 32 random hex chars with bfa_ prefix
  let random_bytes: [u8; 16] = rand::rng().random();
  let raw_key = format!("bfa_{}", hex::encode(random_bytes));

  // SHA-256 hash for storage
  let mut hasher = Sha256::new();
  hasher.update(raw_key.as_bytes());
  let key_hash = hex::encode(hasher.finalize());

  let key_prefix = &raw_key[..12]; // "bfa_" + first 8 hex chars

  // INSERT OR REPLACE to enforce one-key-per-user
  const INSERT_USER_SQL: &str = "INSERT INTO users(user_id) VALUES (?1) ON CONFLICT(user_id) DO NOTHING";
  const UPSERT_KEY_SQL: &str = r#"
        INSERT INTO api_keys (user_id, key_hash, key_prefix)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(user_id) DO UPDATE SET
            key_hash = excluded.key_hash,
            key_prefix = excluded.key_prefix,
            created_at = strftime('%Y-%m-%d %H:%M:%S', 'now')
        "#;
  match database {
    Database::Sqlite(pool) => {
      let mut transaction = pool.begin().await?;
      sqlx::query(INSERT_USER_SQL).bind(user_id as i64).execute(&mut *transaction).await?;
      sqlx::query(UPSERT_KEY_SQL).bind(user_id as i64).bind(&key_hash).bind(key_prefix).execute(&mut *transaction).await?;
      transaction.commit().await?;
    }
    Database::D1(database) => {
      database.batch(vec![(INSERT_USER_SQL, vec![serde_json::json!(user_id)]), (UPSERT_KEY_SQL, vec![serde_json::json!(user_id), serde_json::json!(key_hash), serde_json::json!(key_prefix)])]).await?;
    }
  }
  Ok(raw_key)
}

/// Look up a user_id from a raw API key by hashing it and querying the DB.
pub async fn get_user_id_by_api_key(database: &Database, raw_key: &str) -> Option<u64> {
  use sha2::{Digest, Sha256};

  let mut hasher = Sha256::new();
  hasher.update(raw_key.as_bytes());
  let key_hash = hex::encode(hasher.finalize());

  get_user_id_by_api_key_hash(database, &key_hash).await
}

/// Get API key display info (prefix + created_at) for a user.
pub async fn get_api_key_info(database: &Database, user_id: u64) -> Option<(String, String)> {
  const SQL: &str = "SELECT key_prefix, created_at FROM api_keys WHERE user_id = ?1";
  match database {
    Database::Sqlite(pool) => sqlx::query_as::<_, (String, String)>(SQL).bind(user_id as i64).fetch_optional(pool).await.ok().flatten(),
    Database::D1(database) => database.fetch_all(SQL, vec![serde_json::json!(user_id)]).await.ok()?.first().and_then(|row| Some((json_string(row, "key_prefix")?, json_string(row, "created_at")?))),
  }
}

/// Get the username for a user_id from the users table.
pub async fn get_username_by_user_id(database: &Database, user_id: u64) -> Option<String> {
  const SQL: &str = "SELECT username FROM users WHERE user_id = ?1";
  match database {
    Database::Sqlite(pool) => sqlx::query_as::<_, (Option<String>,)>(SQL).bind(user_id as i64).fetch_optional(pool).await.ok().flatten().and_then(|(name,)| name),
    Database::D1(database) => database.fetch_all(SQL, vec![serde_json::json!(user_id)]).await.ok()?.first().and_then(|row| json_string(row, "username")),
  }
}

pub async fn is_api_key_current(database: &Database, key_hash: &str, user_id: u64) -> bool {
  const SQL: &str = "SELECT user_id FROM api_keys WHERE key_hash = ?1 AND user_id = ?2";
  match database {
    Database::Sqlite(pool) => sqlx::query_as::<_, (i64,)>(SQL).bind(key_hash).bind(user_id as i64).fetch_optional(pool).await.ok().flatten().is_some(),
    Database::D1(database) => database.fetch_all(SQL, vec![serde_json::json!(key_hash), serde_json::json!(user_id)]).await.ok().and_then(|rows| rows.first().cloned()).is_some(),
  }
}

pub async fn set_chart_token(database: &Database, user_id: u64, token: &str) -> crate::models::AppResult<()> {
  const SQL: &str = "UPDATE users SET chart_token = ?2 WHERE user_id = ?1";
  match database {
    Database::Sqlite(pool) => {
      sqlx::query(SQL).bind(user_id as i64).bind(token).execute(pool).await?;
      Ok(())
    }
    Database::D1(database) => database.execute(SQL, vec![serde_json::json!(user_id), serde_json::json!(token)]).await,
  }
}

pub async fn get_chart_token(database: &Database, user_id: u64) -> crate::models::AppResult<Option<String>> {
  const SQL: &str = "SELECT chart_token FROM users WHERE user_id = ?1";
  match database {
    Database::Sqlite(pool) => Ok(sqlx::query_as::<_, (Option<String>,)>(SQL).bind(user_id as i64).fetch_optional(pool).await?.and_then(|(token,)| token)),
    Database::D1(database) => Ok(database.fetch_all(SQL, vec![serde_json::json!(user_id)]).await?.first().and_then(|row| json_string(row, "chart_token"))),
  }
}

pub async fn get_user_id_by_chart_token(database: &Database, token: &str) -> Option<u64> {
  const SQL: &str = "SELECT user_id FROM users WHERE chart_token = ?1";
  match database {
    Database::Sqlite(pool) => sqlx::query_as::<_, (i64,)>(SQL).bind(token).fetch_optional(pool).await.ok().flatten().map(|(user_id,)| user_id as u64),
    Database::D1(database) => database.fetch_all(SQL, vec![serde_json::json!(token)]).await.ok()?.first().and_then(|row| json_i64(row, "user_id")).map(|user_id| user_id as u64),
  }
}

pub struct UserSnapshotData {
  pub has_profile: bool,
  pub model: Option<u8>,
  pub schedule: Option<String>,
}

pub async fn get_user_snapshot(database: &Database, user_id: u64) -> crate::models::AppResult<UserSnapshotData> {
  const SQL: &str = "SELECT bazi_four_pillars IS NOT NULL AS has_profile, llm_model, schedule FROM users WHERE user_id = ?1";
  match database {
    Database::Sqlite(pool) => {
      let row = sqlx::query_as::<_, (bool, Option<u8>, Option<String>)>(SQL).bind(user_id as i64).fetch_optional(pool).await?;
      let (has_profile, model, schedule) = row.unwrap_or((false, None, None));
      Ok(UserSnapshotData { has_profile, model, schedule })
    }
    Database::D1(database) => {
      let rows = database.fetch_all(SQL, vec![serde_json::json!(user_id)]).await?;
      let data = rows.first().map_or(UserSnapshotData { has_profile: false, model: None, schedule: None }, |row| UserSnapshotData {
        has_profile: json_bool(row, "has_profile").unwrap_or(false),
        model: json_u8(row, "llm_model"),
        schedule: json_string(row, "schedule"),
      });
      Ok(data)
    }
  }
}

async fn get_user_id_by_api_key_hash(database: &Database, key_hash: &str) -> Option<u64> {
  const SQL: &str = "SELECT user_id FROM api_keys WHERE key_hash = ?1";
  match database {
    Database::Sqlite(pool) => sqlx::query_as::<_, (i64,)>(SQL).bind(key_hash).fetch_optional(pool).await.ok().flatten().map(|(user_id,)| user_id as u64),
    Database::D1(database) => database.fetch_all(SQL, vec![serde_json::json!(key_hash)]).await.ok()?.first().and_then(|row| json_i64(row, "user_id")).map(|user_id| user_id as u64),
  }
}

fn json_string(row: &serde_json::Map<String, serde_json::Value>, column: &str) -> Option<String> {
  row.get(column).and_then(serde_json::Value::as_str).map(str::to_owned)
}

fn json_i64(row: &serde_json::Map<String, serde_json::Value>, column: &str) -> Option<i64> {
  row.get(column).and_then(|value| value.as_i64().or_else(|| value.as_str().and_then(|value| value.parse().ok())))
}

fn json_u8(row: &serde_json::Map<String, serde_json::Value>, column: &str) -> Option<u8> {
  json_i64(row, column).and_then(|value| u8::try_from(value).ok())
}

fn json_bool(row: &serde_json::Map<String, serde_json::Value>, column: &str) -> Option<bool> {
  row.get(column).and_then(|value| value.as_bool().or_else(|| value.as_i64().map(|value| value != 0)))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn defaults_to_sqlite_when_d1_is_not_configured() {
    let database = init_database("sqlite::memory:", None, reqwest::Client::new()).await.expect("SQLite fallback should initialize");

    assert!(matches!(database, Database::Sqlite(_)));
    update_user_llm_model(&database, 42, 1).await.expect("SQLite fallback should accept writes");
    assert_eq!(get_user_profile(&database, 42).await.llm_model, LlmModel::from_u8(1));
  }
}
