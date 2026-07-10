use crate::models::common::LlmModel;
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use std::hash::{BuildHasher, Hasher};
use std::str::FromStr;
use tracing::{error, info};

/// Generates a time-based ID: timestamp_millis * 1000 + random(0..999)
/// Uses RandomState for a fast, cheap, 0-dependency random number.
fn generate_time_id() -> i64 {
    let now = chrono::Utc::now();
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_i64(now.timestamp_nanos_opt().unwrap_or(0));
    let rand_val = hasher.finish() % 1000;
    now.timestamp_millis() * 1000 + rand_val as i64
}

pub async fn init_db(db_url: &str) -> Result<SqlitePool, sqlx::Error> {
    let options = SqliteConnectOptions::from_str(db_url)?.create_if_missing(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    // Automatically apply any pending migrations
    // If we hit a VersionMismatch, it usually means the DB state and the filesystem are out of sync.
    // We handle this by dropping the metadata table (safe since our SQL uses IF NOT EXISTS).
    if let Err(e) = sqlx::migrate!("./migrations").run(&pool).await {
        error!(
            "Initial migration failed: {}. Attempting metadata reset...",
            e
        );

        // Attempt to Drop the migrations table to force a resync
        let _ = sqlx::query("DROP TABLE IF EXISTS _sqlx_migrations")
            .execute(&pool)
            .await;

        // Retry migration
        sqlx::migrate!("./migrations").run(&pool).await?;
        info!("Migrations resynced successfully after metadata reset.");
    } else {
        info!("Database migrations applied successfully.");
    }

    Ok(pool)
}

pub async fn upsert_user_bazi(
    pool: &SqlitePool,
    user_id: u64,
    username: Option<&str>,
    bazi_four_pillars: &str,
    gender: u8,
    birth_datetime: &str,
) {
    let result = sqlx::query(
        r#"
        INSERT INTO users (user_id, username, bazi_four_pillars, gender, birth_datetime)
        VALUES (?1, ?2, jsonb(?3), ?4, ?5)
        ON CONFLICT(user_id) DO UPDATE SET
            username = excluded.username,
            bazi_four_pillars = excluded.bazi_four_pillars,
            gender = excluded.gender,
            birth_datetime = excluded.birth_datetime
        "#,
    )
    .bind(user_id as i64)
    .bind(username)
    .bind(bazi_four_pillars)
    .bind(gender)
    .bind(birth_datetime)
    .execute(pool)
    .await;

    if let Err(e) = result {
        error!("Failed to save user bazi four pillars: {}", e);
    }
}

pub async fn save_user_bazi_analysis(pool: &SqlitePool, user_id: u64, reading: &str) {
    let result = sqlx::query(
        r#"
        UPDATE users SET bazi_analysis = ?2
        WHERE user_id = ?1
        "#,
    )
    .bind(user_id as i64)
    .bind(reading)
    .execute(pool)
    .await;

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

type UserProfileRow = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<u8>,
    Option<String>,
);

pub async fn get_user_profile(pool: &SqlitePool, user_id: u64) -> UserProfileData {
    let row: Option<UserProfileRow> = sqlx::query_as(r#"SELECT json(bazi_four_pillars), bazi_analysis, bazi_summary, llm_model, schedule FROM users WHERE user_id = ?1"#)
        .bind(user_id as i64)
        .fetch_optional(pool)
        .await
        .unwrap_or_else(|e| {
            error!("Failed to fetch user profile for {}: {}", user_id, e);
            None
        });

    match row {
        Some(r) => UserProfileData {
            bazi_four_pillars: r.0,
            bazi_analysis: r.1,
            bazi_summary: r.2,
            llm_model: r.3.and_then(LlmModel::from_u8),
            schedule: r.4,
        },
        None => UserProfileData {
            bazi_four_pillars: None,
            bazi_analysis: None,
            bazi_summary: None,
            llm_model: None,
            schedule: None,
        },
    }
}

/// Fetch all users who have a non-null schedule for scheduled fortune generation.
pub async fn get_all_scheduled_users(pool: &SqlitePool) -> Vec<(u64, String)> {
    let rows = sqlx::query_as::<_, (i64, String)>(r#"SELECT user_id, schedule FROM users WHERE schedule IS NOT NULL AND schedule != '' AND bazi_four_pillars IS NOT NULL AND bazi_four_pillars != ''"#)
        .fetch_all(pool)
        .await
        .unwrap_or_else(|e| {
            error!("Failed to fetch scheduled users: {}", e);
            Vec::new()
        });

    rows.into_iter()
        .map(|(user_id, schedule)| (user_id as u64, schedule))
        .collect()
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
pub async fn save_llm_log(pool: &SqlitePool, params: LlmLogParams<'_>) {
    let result = sqlx::query(
        r#"
        INSERT INTO llm_logs (id, model, user_id, request_type, request_body, response_body, total_tokens, duration_ms, is_success)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        "#,
    )
    .bind(generate_time_id())
    .bind(params.model)
    .bind(params.user_id)
    .bind(params.request_type)
    .bind(params.request_body)
    .bind(params.response_body)
    .bind(params.total_tokens)
    .bind(params.duration_ms)
    .bind(params.is_success as i32)
    .execute(pool)
    .await;

    if let Err(e) = result {
        error!("Failed to save LLM log: {}", e);
    }
}

pub async fn update_user_llm_model(pool: &SqlitePool, user_id: u64, llm_model: u8) {
    let result = sqlx::query(
        r#"
        INSERT INTO users (user_id, llm_model)
        VALUES (?1, ?2)
        ON CONFLICT(user_id) DO UPDATE SET
            llm_model = excluded.llm_model
        "#,
    )
    .bind(user_id as i64)
    .bind(llm_model)
    .execute(pool)
    .await;

    if let Err(e) = result {
        error!("Failed to update user LLM model: {}", e);
    }
}

pub async fn save_user_bazi_summary(pool: &SqlitePool, user_id: u64, summary: &str) {
    let result = sqlx::query(
        r#"
        UPDATE users SET bazi_summary = ?2
        WHERE user_id = ?1
        "#,
    )
    .bind(user_id as i64)
    .bind(summary)
    .execute(pool)
    .await;

    if let Err(e) = result {
        error!("Failed to save bazi_summary: {}", e);
    }
}

pub async fn update_user_schedule(
    pool: &SqlitePool,
    user_id: u64,
    schedule: Option<&str>,
) -> crate::models::error::AppResult<()> {
    let result = sqlx::query(
        r#"
        UPDATE users SET schedule = ?2
        WHERE user_id = ?1
        "#,
    )
    .bind(user_id as i64)
    .bind(schedule)
    .execute(pool)
    .await;

    if let Err(e) = &result {
        error!("Failed to update user schedule: {}", e);
    }

    result.map(|_| ()).map_err(|e| e.into())
}

// ─────────────────────────────────────────────
// API Key Management
// ─────────────────────────────────────────────

/// Generate a new API key for the user. Revokes any existing key.
/// Returns the raw key (shown once to user). Only the SHA-256 hash is stored.
pub async fn create_api_key(pool: &SqlitePool, user_id: u64) -> crate::models::AppResult<String> {
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
    sqlx::query(
        r#"
        INSERT INTO api_keys (user_id, key_hash, key_prefix)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(user_id) DO UPDATE SET
            key_hash = excluded.key_hash,
            key_prefix = excluded.key_prefix,
            created_at = strftime('%Y-%m-%d %H:%M:%S', 'now')
        "#,
    )
    .bind(user_id as i64)
    .bind(&key_hash)
    .bind(key_prefix)
    .execute(pool)
    .await?;

    Ok(raw_key)
}

/// Look up a user_id from a raw API key by hashing it and querying the DB.
pub async fn get_user_id_by_api_key(pool: &SqlitePool, raw_key: &str) -> Option<u64> {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(raw_key.as_bytes());
    let key_hash = hex::encode(hasher.finalize());

    sqlx::query_as::<_, (i64,)>("SELECT user_id FROM api_keys WHERE key_hash = ?1")
        .bind(&key_hash)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .map(|(uid,)| uid as u64)
}

/// Get API key display info (prefix + created_at) for a user.
pub async fn get_api_key_info(pool: &SqlitePool, user_id: u64) -> Option<(String, String)> {
    sqlx::query_as::<_, (String, String)>(
        "SELECT key_prefix, created_at FROM api_keys WHERE user_id = ?1",
    )
    .bind(user_id as i64)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
}

/// Get the username for a user_id from the users table.
pub async fn get_username_by_user_id(pool: &SqlitePool, user_id: u64) -> Option<String> {
    sqlx::query_as::<_, (Option<String>,)>("SELECT username FROM users WHERE user_id = ?1")
        .bind(user_id as i64)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .and_then(|(name,)| name)
}
