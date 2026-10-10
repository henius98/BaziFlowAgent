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

/// Generates a time-based ID: timestamp_millis * 1000 + sequence(0..999)
/// Guarantees monotonic ordering and collision resistance under bursts.
pub fn generate_time_id() -> i64 {
  use std::sync::Mutex;
  static STATE: Mutex<(i64, u32)> = Mutex::new((0, 0));

  let now_ms = chrono::Utc::now().timestamp_millis();
  let mut state = STATE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

  if now_ms <= state.0 {
    state.1 = (state.1 + 1) % 1000;
  } else {
    state.0 = now_ms;
    state.1 = 0;
  }

  state.0 * 1000 + (state.1 as i64)
}

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
  pub rate_limit: Option<usize>,
}

type UserProfileRow = (Option<String>, Option<String>, Option<String>, Option<i64>, Option<String>, Option<String>, Option<i64>);

pub async fn get_user_profile(database: &Database, user_id: u64) -> UserProfileData {
  const SQL: &str = r#"
        SELECT
            json(users.bazi_four_pillars) AS bazi_four_pillars,
            users.bazi_analysis,
            users.bazi_summary,
            llm_models.model_id AS llm_model_id,
            llm_models.model_name AS llm_model_name,
            users.schedule,
            users.rate_limit
        FROM users
        LEFT JOIN llm_models
            ON llm_models.model_id = users.llm_model
            AND llm_models.is_enabled = 1
        WHERE users.user_id = ?1
        "#;
  let row: Option<UserProfileRow> = match database {
    Database::Sqlite(pool) => sqlx::query_as(SQL).bind(user_id as i64).fetch_optional(pool).await.map_err(Into::into),
    Database::D1(database) => database.fetch_all(SQL, vec![serde_json::json!(user_id)]).await.map(|rows| {
      rows.first().map(|row| {
        (
          json_string(row, "bazi_four_pillars"),
          json_string(row, "bazi_analysis"),
          json_string(row, "bazi_summary"),
          json_i64(row, "llm_model_id"),
          json_string(row, "llm_model_name"),
          json_string(row, "schedule"),
          json_i64(row, "rate_limit"),
        )
      })
    }),
  }
  .unwrap_or_else(|e: crate::models::AppError| {
    error!("Failed to fetch user profile for {}: {}", user_id, e);
    None
  });

  match row {
    Some((pillars, analysis, summary, Some(model_id), Some(model_name), sched, limit)) => {
      let model = u8::try_from(model_id).ok().map(|id| LlmModel::new(id, model_name, true));
      UserProfileData { bazi_four_pillars: pillars, bazi_analysis: analysis, bazi_summary: summary, llm_model: model, schedule: sched, rate_limit: limit.and_then(|l| usize::try_from(l).ok()) }
    }
    Some((pillars, analysis, summary, _, _, sched, limit)) => {
      UserProfileData { bazi_four_pillars: pillars, bazi_analysis: analysis, bazi_summary: summary, llm_model: None, schedule: sched, rate_limit: limit.and_then(|l| usize::try_from(l).ok()) }
    }
    None => UserProfileData { bazi_four_pillars: None, bazi_analysis: None, bazi_summary: None, llm_model: None, schedule: None, rate_limit: None },
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
  pub request_type: &'a str,
  pub request_body: &'a str,
  pub response_body: &'a str,
  pub total_tokens: Option<i64>,
  pub duration_ms: i64,
  pub is_success: bool,
  pub model_id: Option<i64>,
}

/// Persist an LLM call log entry (request + response or error).
pub async fn save_llm_log(database: &Database, params: LlmLogParams<'_>) {
  let log_id = generate_time_id();
  const SQL: &str = r#"
        INSERT INTO llm_logs (id, model, user_id, request_type, request_body, response_body, total_tokens, duration_ms, is_success, model_id)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        "#;
  let result = match database {
    Database::Sqlite(pool) => sqlx::query(SQL)
      .bind(log_id)
      .bind(params.model)
      .bind(params.user_id)
      .bind(params.request_type)
      .bind(params.request_body)
      .bind(params.response_body)
      .bind(params.total_tokens)
      .bind(params.duration_ms)
      .bind(params.is_success as i32)
      .bind(params.model_id)
      .execute(pool)
      .await
      .map(|_| ())
      .map_err(Into::into),
    Database::D1(database) => {
      database
        .execute(
          SQL,
          vec![
            serde_json::json!(log_id),
            serde_json::json!(params.model),
            params.user_id.map_or(serde_json::Value::Null, |value| serde_json::json!(value)),
            serde_json::json!(params.request_type),
            serde_json::json!(params.request_body),
            serde_json::json!(params.response_body),
            params.total_tokens.map_or(serde_json::Value::Null, |value| serde_json::json!(value)),
            serde_json::json!(params.duration_ms),
            serde_json::json!(params.is_success as i32),
            params.model_id.map_or(serde_json::Value::Null, |value| serde_json::json!(value)),
          ],
        )
        .await
    }
  };

  if let Err(e) = result {
    error!("Failed to save LLM log: {}", e);
  }
}

pub async fn get_llm_models(database: &Database) -> crate::models::AppResult<Vec<LlmModel>> {
  const SQL: &str = "SELECT model_id, model_name FROM llm_models WHERE is_enabled = 1 ORDER BY model_id ASC";
  let models: Vec<LlmModel> = match database {
    Database::Sqlite(pool) => {
      sqlx::query_as::<_, (i64, String)>(SQL).fetch_all(pool).await?.into_iter().filter_map(|(id, name)| u8::try_from(id).ok().map(|model_id| LlmModel::new(model_id, name, true))).collect()
    }
    Database::D1(database) => database
      .fetch_all(SQL, Vec::new())
      .await?
      .into_iter()
      .filter_map(|row| {
        let id = json_u8(&row, "model_id")?;
        let name = json_string(&row, "model_name")?;
        Some(LlmModel::new(id, name, true))
      })
      .collect(),
  };

  Ok(models)
}

pub async fn get_available_model(database: &Database, model_id: u8) -> crate::models::AppResult<Option<LlmModel>> {
  Ok(get_llm_models(database).await?.into_iter().find(|model| model.id() == model_id))
}

pub async fn update_user_llm_model(database: &Database, user_id: u64, llm_model: u8) -> crate::models::AppResult<LlmModel> {
  let model = get_available_model(database, llm_model).await?.ok_or_else(|| crate::models::AppError::Message("Selected LLM model is unavailable".into()))?;
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
  result?;
  Ok(model)
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
  let key_id = generate_time_id();

  // INSERT OR REPLACE to enforce one-key-per-user
  const INSERT_USER_SQL: &str = "INSERT INTO users(user_id) VALUES (?1) ON CONFLICT(user_id) DO NOTHING";
  const UPSERT_KEY_SQL: &str = r#"
        INSERT INTO api_keys (id, user_id, key_hash, key_prefix)
        VALUES (?1, ?2, ?3, ?4)
        ON CONFLICT(user_id) DO UPDATE SET
            id = excluded.id,
            key_hash = excluded.key_hash,
            key_prefix = excluded.key_prefix
        "#;
  match database {
    Database::Sqlite(pool) => {
      let mut transaction = pool.begin().await?;
      sqlx::query(INSERT_USER_SQL).bind(user_id as i64).execute(&mut *transaction).await?;
      sqlx::query(UPSERT_KEY_SQL).bind(key_id).bind(user_id as i64).bind(&key_hash).bind(key_prefix).execute(&mut *transaction).await?;
      transaction.commit().await?;
    }
    Database::D1(database) => {
      database
        .batch(vec![
          (INSERT_USER_SQL, vec![serde_json::json!(user_id)]),
          (UPSERT_KEY_SQL, vec![serde_json::json!(key_id), serde_json::json!(user_id), serde_json::json!(key_hash), serde_json::json!(key_prefix)]),
        ])
        .await?;
    }
  }
  Ok(raw_key)
}

/// Look up an authenticated user from a raw API key by hashing it and querying the DB, including custom rate limit.
pub async fn get_auth_user_by_api_key(database: &Database, raw_key: &str) -> Option<(u64, Option<usize>)> {
  use sha2::{Digest, Sha256};

  let mut hasher = Sha256::new();
  hasher.update(raw_key.as_bytes());
  let key_hash = hex::encode(hasher.finalize());

  get_auth_user_by_api_key_hash(database, &key_hash).await
}

/// Look up a user_id from a raw API key by hashing it and querying the DB.
pub async fn get_user_id_by_api_key(database: &Database, raw_key: &str) -> Option<u64> {
  get_auth_user_by_api_key(database, raw_key).await.map(|(user_id, _)| user_id)
}

/// Fetch a user's custom rate limit (requests per minute) from the users table.
pub async fn get_user_rate_limit(database: &Database, user_id: u64) -> Option<usize> {
  const SQL: &str = "SELECT rate_limit FROM users WHERE user_id = ?1";
  match database {
    Database::Sqlite(pool) => sqlx::query_as::<_, (Option<i64>,)>(SQL).bind(user_id as i64).fetch_optional(pool).await.ok().flatten().and_then(|(l,)| l.and_then(|v| usize::try_from(v).ok())),
    Database::D1(database) => database.fetch_all(SQL, vec![serde_json::json!(user_id)]).await.ok()?.first().and_then(|row| json_i64(row, "rate_limit").and_then(|l| usize::try_from(l).ok())),
  }
}

/// Set or clear a user's custom rate limit in the users table.
pub async fn set_user_rate_limit(database: &Database, user_id: u64, rate_limit: Option<usize>) -> Result<(), crate::models::AppError> {
  const SQL: &str = "UPDATE users SET rate_limit = ?1 WHERE user_id = ?2";
  let limit_i64 = rate_limit.map(|l| l as i64);
  match database {
    Database::Sqlite(pool) => {
      sqlx::query(SQL).bind(limit_i64).bind(user_id as i64).execute(pool).await?;
    }
    Database::D1(database) => {
      let limit_val = match limit_i64 {
        Some(v) => serde_json::json!(v),
        None => serde_json::Value::Null,
      };
      database.execute(SQL, vec![limit_val, serde_json::json!(user_id)]).await?;
    }
  }
  Ok(())
}

/// Get API key display info (prefix + created_at timestamp_millis) for a user.
pub async fn get_api_key_info(database: &Database, user_id: u64) -> Option<(String, String)> {
  const SQL: &str = "SELECT key_prefix, id FROM api_keys WHERE user_id = ?1";
  match database {
    Database::Sqlite(pool) => sqlx::query_as::<_, (String, i64)>(SQL).bind(user_id as i64).fetch_optional(pool).await.ok().flatten().map(|(prefix, id)| (prefix, (id / 1000).to_string())),
    Database::D1(database) => {
      database.fetch_all(SQL, vec![serde_json::json!(user_id)]).await.ok()?.first().and_then(|row| Some((json_string(row, "key_prefix")?, (json_i64(row, "id")? / 1000).to_string())))
    }
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

/// Fetch user's last_active_at unix timestamp (seconds).
pub async fn get_user_last_active(database: &Database, user_id: u64) -> crate::models::AppResult<Option<i64>> {
  const SQL: &str = "SELECT last_active_at FROM users WHERE user_id = ?1";
  match database {
    Database::Sqlite(pool) => Ok(sqlx::query_as::<_, (i64,)>(SQL).bind(user_id as i64).fetch_optional(pool).await?.map(|(ts,)| ts)),
    Database::D1(database) => Ok(database.fetch_all(SQL, vec![serde_json::json!(user_id)]).await?.first().and_then(|row| json_i64(row, "last_active_at"))),
  }
}

/// Fetch user's last_active_at formatted as standard "%Y-%m-%d %H:%M:%S" from v_users.
pub async fn get_user_last_active_formatted(database: &Database, user_id: u64) -> crate::models::AppResult<Option<String>> {
  const SQL: &str = "SELECT last_active_at FROM v_users WHERE user_id = ?1";
  match database {
    Database::Sqlite(pool) => Ok(sqlx::query_as::<_, (String,)>(SQL).bind(user_id as i64).fetch_optional(pool).await?.map(|(dt,)| dt)),
    Database::D1(database) => Ok(database.fetch_all(SQL, vec![serde_json::json!(user_id)]).await?.first().and_then(|row| json_string(row, "last_active_at"))),
  }
}

/// Update user's last_active_at timestamp to current unix epoch.
pub async fn touch_user_last_active(database: &Database, user_id: u64) -> crate::models::AppResult<()> {
  const SQL: &str = "UPDATE users SET last_active_at = unixepoch() WHERE user_id = ?1";
  match database {
    Database::Sqlite(pool) => {
      sqlx::query(SQL).bind(user_id as i64).execute(pool).await?;
    }
    Database::D1(database) => {
      database.execute(SQL, vec![serde_json::json!(user_id)]).await?;
    }
  }
  Ok(())
}

async fn get_auth_user_by_api_key_hash(database: &Database, key_hash: &str) -> Option<(u64, Option<usize>)> {
  const SQL: &str = "SELECT api_keys.user_id, users.rate_limit FROM api_keys LEFT JOIN users ON users.user_id = api_keys.user_id WHERE api_keys.key_hash = ?1";
  match database {
    Database::Sqlite(pool) => sqlx::query_as::<_, (i64, Option<i64>)>(SQL)
      .bind(key_hash)
      .fetch_optional(pool)
      .await
      .ok()
      .flatten()
      .map(|(user_id, rate_limit)| (user_id as u64, rate_limit.and_then(|l| usize::try_from(l).ok()))),
    Database::D1(database) => database.fetch_all(SQL, vec![serde_json::json!(key_hash)]).await.ok()?.first().and_then(|row| {
      let user_id = json_i64(row, "user_id")? as u64;
      let rate_limit = json_i64(row, "rate_limit").and_then(|l| usize::try_from(l).ok());
      Some((user_id, rate_limit))
    }),
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
    let profile = get_user_profile(&database, 42).await;
    assert_eq!(profile.llm_model.as_ref().map(|m| m.as_str()), Some("google/gemini-3.8-flash"));
  }

  #[tokio::test]
  async fn llm_models_control_model_selection_and_effective_preference() {
    let pool = init_db("sqlite::memory:").await.expect("database");
    let database = Database::Sqlite(pool.clone());

    let models = get_llm_models(&database).await.expect("available models");
    assert_eq!(models.len(), 2);
    assert_eq!(models[0].id(), 1);
    assert_eq!(models[0].as_str(), "google/gemini-3.8-flash");
    assert_eq!(models[1].id(), 2);
    assert_eq!(models[1].as_str(), "google/gemini-3.1-pro-preview");

    update_user_llm_model(&database, 42, 1).await.expect("enabled model should be saved");
    let profile = get_user_profile(&database, 42).await;
    assert_eq!(profile.llm_model.as_ref().map(|m| m.as_str()), Some("google/gemini-3.8-flash"));

    sqlx::query("UPDATE llm_models SET is_enabled = 0 WHERE model_id = 1").execute(&pool).await.expect("disable model");

    let updated_models = get_llm_models(&database).await.expect("available models");
    assert!(!updated_models.iter().any(|m| m.id() == 1));
    assert!(update_user_llm_model(&database, 43, 1).await.is_err());
    assert_eq!(get_user_profile(&database, 42).await.llm_model, None);

    // Verify AUTOINCREMENT assigns the next ID monotonically when a new model is added
    sqlx::query("INSERT INTO llm_models (model_name, is_enabled) VALUES (?1, 1)").bind("openai/gpt-5.5-pro").execute(&pool).await.expect("insert new model");
    let updated_models = get_llm_models(&database).await.expect("available models");
    assert_eq!(updated_models.iter().find(|m| m.as_str() == "openai/gpt-5.5-pro").map(|m| m.id()), Some(3));
  }

  #[tokio::test]
  async fn user_rate_limit_can_be_stored_and_retrieved() {
    let pool = init_db("sqlite::memory:").await.expect("database");
    let database = Database::Sqlite(pool.clone());
    let user_id = 999;

    sqlx::query("INSERT INTO users (user_id) VALUES (?1)").bind(user_id as i64).execute(&pool).await.expect("insert user");
    assert_eq!(get_user_rate_limit(&database, user_id).await, None);

    set_user_rate_limit(&database, user_id, Some(15)).await.expect("set rate limit");
    assert_eq!(get_user_rate_limit(&database, user_id).await, Some(15));

    let key = create_api_key(&database, user_id).await.expect("api key");
    let auth_info = get_auth_user_by_api_key(&database, &key).await.expect("auth user");
    assert_eq!(auth_info.0, user_id);
    assert_eq!(auth_info.1, Some(15));

    set_user_rate_limit(&database, user_id, None).await.expect("clear rate limit");
    assert_eq!(get_user_rate_limit(&database, user_id).await, None);
    let auth_info_cleared = get_auth_user_by_api_key(&database, &key).await.expect("auth user");
    assert_eq!(auth_info_cleared.1, None);
  }

  #[tokio::test]
  async fn time_id_and_api_key_info_represent_creation_time() {
    let pool = init_db("sqlite::memory:").await.expect("database");
    let database = Database::Sqlite(pool.clone());
    let user_id = 12345;

    let before_ms = chrono::Utc::now().timestamp_millis();
    let key = create_api_key(&database, user_id).await.expect("create api key");
    let after_ms = chrono::Utc::now().timestamp_millis();

    let (prefix, created_at_ms_str) = get_api_key_info(&database, user_id).await.expect("api key info");
    assert_eq!(prefix, &key[..12]);

    let created_at_ms: i64 = created_at_ms_str.parse().expect("valid millisecond timestamp");
    assert!(created_at_ms >= before_ms && created_at_ms <= after_ms);

    // Verify monotonic IDs under rapid calls
    let id1 = generate_time_id();
    let id2 = generate_time_id();
    let id3 = generate_time_id();
    assert!(id1 < id2);
    assert!(id2 < id3);
  }

  #[tokio::test]
  async fn users_last_active_at_stored_as_timestamp_and_view_formats_as_utc8() {
    let pool = init_db("sqlite::memory:").await.expect("database");
    let database = Database::Sqlite(pool.clone());
    let user_id = 8888;

    let before_epoch = chrono::Utc::now().timestamp();
    sqlx::query("INSERT INTO users (user_id) VALUES (?1)").bind(user_id as i64).execute(&pool).await.expect("insert user");
    let after_epoch = chrono::Utc::now().timestamp();

    let ts = get_user_last_active(&database, user_id).await.expect("fetch last active").expect("timestamp exists");
    assert!(ts >= before_epoch && ts <= after_epoch);

    let formatted = get_user_last_active_formatted(&database, user_id).await.expect("fetch formatted").expect("formatted string exists");
    assert_eq!(crate::utils::format_timestamp_utc8(ts), Some(formatted.clone()));
    assert_eq!(crate::utils::parse_timestamp_utc8(&formatted), Some(ts));
  }
}
