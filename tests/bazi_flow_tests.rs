use baziflow_agent::config::AppConfig;
use baziflow_agent::models::AppState;
use baziflow_agent::services::bazi_service::*;
use mockito::Server;
use sqlx::SqlitePool;
use std::sync::Arc;

#[tokio::test]
async fn test_core_bazi_analysis() {
    let pool = SqlitePool::connect("sqlite::memory:").await.expect("Failed to connect to memory db");
    sqlx::migrate!().run(&pool).await.expect("Failed to run migrations");

    let mut server = Server::new_async().await;
    let mock_url = server.url();

    server.mock("GET", mockito::Matcher::Regex(r"^/getbasebz8\.php.*".to_string()))
        .with_status(200)
        .with_body(r#"{"info":{"gender":"男,乾造","solar_date":"1990-01-01 00:00:00","lunisolar_date":"一九八九年十二月初五日子时"},"bz":{"year_steam":"己","year_branch":"巳","month_steam":"丙","month_branch":"子","day_steam":"丁","day_branch":"丑","hour_steam":"庚","hour_branch":"子"},"dyshensha":[],"lnshensha":[]}"#)
        .create_async().await;

    server
        .mock("GET", mockito::Matcher::Regex(r"^/getRysl\.php.*".to_string()))
        .with_status(200)
        .with_body(r#"{"data":"some yongshi"}"#)
        .create_async()
        .await;

    server
        .mock("GET", mockito::Matcher::Regex(r"^/getGZRelaction3\.php.*".to_string()))
        .with_status(200)
        .with_body(r#"[["relation1"]]"#)
        .create_async()
        .await;

    server
        .mock("GET", mockito::Matcher::Regex(r"^/getliunianshensha5\.php.*".to_string()))
        .with_status(200)
        .with_body(r#"{"shensha":[[],["shensha1"]]}"#)
        .create_async()
        .await;

    server.mock("POST", "/chat/completions")
        .with_status(200)
        .with_header("content-type", "text/event-stream")
        .with_body("data: {\"id\":\"chatcmpl-123\",\"object\":\"chat.completion.chunk\",\"created\":1694268190,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello World\"},\"finish_reason\":null}]}\n\ndata: [DONE]\n\n")
        .create_async().await;

    let mut config = AppConfig::from_env().unwrap_or_else(|_| AppConfig {
        telegram_bot_token: "".into(),
        llm_client_config: baziflow_agent::services::llm::LlmClientConfig {
            api_key: "test".into(),
            api_base: mock_url.clone(),
            timeout_seconds: 30,
            http_client: None,
        },
        llm_model_name: "gpt-4o".into(),
        database_url: "sqlite::memory:".into(),
        user_contexts_expiration_minutes: 60,
        context_cleanup_cron: "".into(),
        log_cleanup_cron: "".into(),
        log_retention_days: 7,
        max_context_messages: 10,
        base_url: "http://localhost".into(),
        log_level: "info".into(),
        r2_account_id: None,
        r2_access_key_id: None,
        r2_secret_access_key: None,
        r2_bucket_name: None,
    });
    config.llm_client_config.api_base = mock_url.clone();

    let state = Arc::new(AppState::new(reqwest::Client::new(), pool.clone(), Arc::new(config)));
    let _ = tokio::fs::create_dir_all("public").await;

    let params = BaziDataParams {
        user_id: 123,
        username: "TestUser",
        birth_date: "1990-01-01",
        birth_hour: 0,
        birth_minute: 0,
        gender: 1,
        location: None,
    };
    let structured_data = prepare_bazi_data(&state, params).await.expect("Failed to prepare bazi data");

    let receiver = core_bazi_analysis(&state, 123 as u64, &structured_data, None::<baziflow_agent::models::common::LlmModel>)
        .await
        .expect("Failed to run core analysis");
    let mut rx = receiver;
    let mut out = String::new();
    while let Some(chunk) = rx.recv().await {
        out.push_str(&chunk);
    }
    assert_eq!(out, "Hello World");
}
