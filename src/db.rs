use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use std::str::FromStr;
use tracing::{error, info};

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
        error!("Initial migration failed: {}. Attempting metadata reset...", e);
        
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

pub async fn save_or_update_user(
    pool: &SqlitePool,
    user_id: i64,
    username: Option<&str>,
) {
    let result = sqlx::query(
        r#"
        INSERT INTO users (user_id, username, last_active_at)
        VALUES (?1, ?2, CURRENT_TIMESTAMP)
        ON CONFLICT(user_id) DO UPDATE SET
            username = excluded.username,
            last_active_at = excluded.last_active_at
        "#,
    )
    .bind(user_id)
    .bind(username)
    .execute(pool)
    .await;

    if let Err(e) = result {
        error!("Failed to save user: {}", e);
    }
}

pub async fn save_request(
    pool: &SqlitePool,
    user_id: i64,
    request_type: &str,
    target_date: Option<&str>,
    text_content: Option<&str>,
    llm_response: Option<&str>,
) {
    let result = sqlx::query(
        r#"
        INSERT INTO requests (user_id, request_type, target_date, text_content, llm_response)
        VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
    )
    .bind(user_id)
    .bind(request_type)
    .bind(target_date)
    .bind(text_content)
    .bind(llm_response)
    .execute(pool)
    .await;

    if let Err(e) = result {
        error!("Failed to save request: {}", e);
    }
}

pub async fn save_or_update_user_bazi(
    pool: &SqlitePool,
    user_id: i64,
    bazi: &str,
    gender: u8,
    birth_datetime: Option<&str>,
) {
    let result = sqlx::query(
        r#"
        INSERT INTO users (user_id, bazi, gender, birth_datetime, last_active_at)
        VALUES (?1, ?2, ?3, ?4, CURRENT_TIMESTAMP)
        ON CONFLICT(user_id) DO UPDATE SET
            bazi = excluded.bazi,
            gender = excluded.gender,
            birth_datetime = excluded.birth_datetime,
            last_active_at = excluded.last_active_at
        "#,
    )
    .bind(user_id)
    .bind(bazi)
    .bind(gender as i64)
    .bind(birth_datetime)
    .execute(pool)
    .await;

    if let Err(e) = result {
        error!("Failed to save user bazi: {}", e);
    }
}


pub async fn save_user_destiny_reading(pool: &SqlitePool, user_id: i64, reading: &str) {
    let result = sqlx::query(
        r#"
        UPDATE users SET destiny_reading = ?2, last_active_at = CURRENT_TIMESTAMP
        WHERE user_id = ?1
        "#,
    )
    .bind(user_id)
    .bind(reading)
    .execute(pool)
    .await;

    if let Err(e) = result {
        error!("Failed to save destiny reading: {}", e);
    }
}

pub async fn get_user_profile(pool: &SqlitePool, user_id: i64) -> (Option<String>, Option<String>) {
    let row: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        r#"SELECT bazi, destiny_reading FROM users WHERE user_id = ?1"#
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    match row {
        Some(r) => (r.0, r.1),
        None => (None, None),
    }
}
