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

    // Automatically apply any pending migrations found in the `migrations` folder
    sqlx::migrate!("./migrations").run(&pool).await?;

    info!("Database initialized and tables verified.");
    Ok(pool)
}

pub async fn save_or_update_user(
    pool: &SqlitePool,
    user_id: i64,
    username: Option<&str>,
    first_name: Option<&str>,
    last_name: Option<&str>,
) {
    let result = sqlx::query(
        r#"
        INSERT INTO users (user_id, username, first_name, last_name, last_active_at)
        VALUES (?1, ?2, ?3, ?4, CURRENT_TIMESTAMP)
        ON CONFLICT(user_id) DO UPDATE SET
            username = excluded.username,
            first_name = excluded.first_name,
            last_name = excluded.last_name,
            last_active_at = excluded.last_active_at
        "#,
    )
    .bind(user_id)
    .bind(username)
    .bind(first_name)
    .bind(last_name)
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
    dify_response: Option<&str>,
) {
    let result = sqlx::query(
        r#"
        INSERT INTO requests (user_id, request_type, target_date, text_content, dify_response)
        VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
    )
    .bind(user_id)
    .bind(request_type)
    .bind(target_date)
    .bind(text_content)
    .bind(dify_response)
    .execute(pool)
    .await;

    if let Err(e) = result {
        error!("Failed to save request: {}", e);
    }
}

pub async fn save_or_update_user_bazi(pool: &SqlitePool, user_id: i64, bazi: &str) {
    let result = sqlx::query(
        r#"
        INSERT INTO users (user_id, bazi, last_active_at)
        VALUES (?1, ?2, CURRENT_TIMESTAMP)
        ON CONFLICT(user_id) DO UPDATE SET
            bazi = excluded.bazi,
            last_active_at = excluded.last_active_at
        "#,
    )
    .bind(user_id)
    .bind(bazi)
    .execute(pool)
    .await;

    if let Err(e) = result {
        error!("Failed to save user bazi: {}", e);
    }
}

pub async fn get_user_bazi(pool: &SqlitePool, user_id: i64) -> Option<String> {
    let row: Option<(Option<String>,)> = sqlx::query_as(
        r#"SELECT bazi FROM users WHERE user_id = ?1"#
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    row.and_then(|r| r.0)
}
