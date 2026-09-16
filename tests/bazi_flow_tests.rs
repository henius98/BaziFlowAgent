mod test_helpers;
use baziflow_agent::services::bazi_service::*;
use mockito::Server;
use sqlx::SqlitePool;

#[tokio::test]
async fn test_core_bazi_analysis() {
  let pool = SqlitePool::connect("sqlite::memory:").await.expect("Failed to connect to memory db");
  sqlx::migrate!().run(&pool).await.expect("Failed to run migrations");

  let mut server = Server::new_async().await;
  let mock_url = server.url();

  server.mock("GET", mockito::Matcher::Regex(r"^/getbasebz8\.php.*".to_string())).with_status(200).with_body(include_str!("fixtures/chart.json")).create_async().await;

  server.mock("GET", mockito::Matcher::Regex(r"^/getRysl\.php.*".to_string())).with_status(200).with_body(r#"{"data":"some yongshi"}"#).create_async().await;

  server.mock("GET", mockito::Matcher::Regex(r"^/getGZRelaction3\.php.*".to_string())).with_status(200).with_body(r#"[["relation1"]]"#).create_async().await;

  server.mock("GET", mockito::Matcher::Regex(r"^/getliunianshensha5\.php.*".to_string())).with_status(200).with_body(r#"{"shensha":[[],["shensha1"]]}"#).create_async().await;

  server.mock("POST", "/chat/completions")
        .with_status(200)
        .with_header("content-type", "text/event-stream")
        .with_body("data: {\"id\":\"chatcmpl-123\",\"object\":\"chat.completion.chunk\",\"created\":1694268190,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello World\"},\"finish_reason\":null}]}\n\ndata: [DONE]\n\n")
        .create_async().await;

  let config = std::sync::Arc::new(test_helpers::test_config(mock_url));
  let state = test_helpers::test_state(config, pool.clone()).await;
  let _ = tokio::fs::create_dir_all("public").await;

  let params = BaziDataParams { user_id: 123, username: "TestUser", birth_date: "1990-01-01", birth_time: None, gender: 1, location: None };
  let structured_data = prepare_bazi_data(&state, params).await.expect("Failed to prepare bazi data");
  assert!(!structured_data.info.birth_time_known);
  assert_eq!(structured_data.info.birth_location, "未知");
  assert!(structured_data.info.solar_date.contains("12:00"));

  let receiver = core_bazi_analysis(&state, 123_u64, &structured_data, None::<baziflow_agent::models::common::LlmModel>).await.expect("Failed to run core analysis");
  let mut rx = receiver;
  let mut out = String::new();
  while let Some(chunk) = rx.recv().await {
    out.push_str(&chunk);
  }
  rx.finish().await.unwrap();
  assert_eq!(out, "Hello World");

  let failed = server.mock("POST", "/chat/completions").with_status(200).with_header("content-type", "text/event-stream").with_body("data: {invalid-json}\n\n").create_async().await;
  let mut broken = core_bazi_analysis(&state, 123, &structured_data, None).await.unwrap();
  while broken.recv().await.is_some() {}
  assert!(broken.finish().await.is_err());
  failed.assert_async().await;
}
