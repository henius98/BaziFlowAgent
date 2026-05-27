use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use std::str::FromStr;
use tracing::{error, info};

pub async fn init_db(db_url: &str) -> Result<SqlitePool, sqlx::Error> {
    let options = SqliteConnectOptions::from_str(db_url)?.create_if_missing(true);

    let pool = SqlitePoolOptions::new().max_connections(5).connect_with(options).await?;

    // Automatically apply any pending migrations
    // If we hit a VersionMismatch, it usually means the DB state and the filesystem are out of sync.
    // We handle this by dropping the metadata table (safe since our SQL uses IF NOT EXISTS).
    if let Err(e) = sqlx::migrate!("./migrations").run(&pool).await {
        error!("Initial migration failed: {}. Attempting metadata reset...", e);

        // Attempt to Drop the migrations table to force a resync
        let _ = sqlx::query("DROP TABLE IF EXISTS _sqlx_migrations").execute(&pool).await;

        // Retry migration
        sqlx::migrate!("./migrations").run(&pool).await?;
        info!("Migrations resynced successfully after metadata reset.");
    } else {
        info!("Database migrations applied successfully.");
    }

    Ok(pool)
}

pub async fn save_request(pool: &SqlitePool, user_id: u64, request_type: &str, target_date: Option<&str>, text_content: Option<&str>, llm_response: Option<&str>) {
    let result = sqlx::query(
        r#"
        INSERT INTO requests (user_id, request_type, target_date, text_content, llm_response)
        VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
    )
    .bind(user_id as i64)
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

pub async fn save_or_update_user_bazi_four_pillars(pool: &SqlitePool, user_id: u64, bazi_four_pillars: &str, gender: u8, birth_datetime: Option<&str>) {
    let result = sqlx::query(
        r#"
        INSERT INTO users (user_id, bazi_four_pillars, gender, birth_datetime)
        VALUES (?1, jsonb(?2), ?3, ?4)
        ON CONFLICT(user_id) DO UPDATE SET
            bazi_four_pillars = excluded.bazi_four_pillars,
            gender = excluded.gender,
            birth_datetime = excluded.birth_datetime
        "#,
    )
    .bind(user_id as i64)
    .bind(bazi_four_pillars)
    .bind(gender)
    .bind(birth_datetime)
    .execute(pool)
    .await;

    if let Err(e) = result {
        error!("Failed to save user bazi four pillars: {}", e);
    }
}

pub async fn save_user_destiny_reading(pool: &SqlitePool, user_id: u64, reading: &str) {
    let result = sqlx::query(
        r#"
        UPDATE users SET destiny_reading = ?2
        WHERE user_id = ?1
        "#,
    )
    .bind(user_id as i64)
    .bind(reading)
    .execute(pool)
    .await;

    if let Err(e) = result {
        error!("Failed to save destiny reading: {}", e);
    }
}

pub async fn get_user_profile(pool: &SqlitePool, user_id: u64) -> (Option<String>, Option<String>) {
    let row: Option<(Option<String>, Option<String>)> = sqlx::query_as(r#"SELECT json(bazi_four_pillars), destiny_reading FROM users WHERE user_id = ?1"#)
        .bind(user_id as i64)
        .fetch_optional(pool)
        .await
        .unwrap_or_else(|e| {
            error!("Failed to fetch user profile for {}: {}", user_id, e);
            None
        });

    match row {
        Some(r) => (r.0, r.1),
        None => (None, None),
    }
}

/// Fetch all users who have a non-null bazi_four_pillars profile for scheduled fortune generation.
pub async fn get_all_users_with_bazi(pool: &SqlitePool) -> Vec<(u64, String, Option<String>)> {
    let rows =
        sqlx::query_as::<_, (i64, String, Option<String>)>(r#"SELECT user_id, json(bazi_four_pillars), destiny_reading FROM users WHERE bazi_four_pillars IS NOT NULL AND bazi_four_pillars != ''"#)
            .fetch_all(pool)
            .await
            .unwrap_or_else(|e| {
                error!("Failed to fetch users with bazi profiles: {}", e);
                Vec::new()
            });

    rows.into_iter().map(|(user_id, bazi, destiny)| (user_id as u64, bazi, destiny)).collect()
}
