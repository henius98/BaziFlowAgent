use async_openai::types::chat::ChatCompletionRequestUserMessageArgs;
use baziflow_agent::{
  config::RuntimeConfig,
  models::{LlmRequestType, runtime::Runtime},
  repos::{self, Database},
  services::llm::{LlmClientConfig, LlmRequestParams, call_llm},
};
use sqlx::SqlitePool;

async fn test_database() -> SqlitePool {
  let pool = SqlitePool::connect("sqlite::memory:").await.expect("test database should open");
  sqlx::migrate!().run(&pool).await.expect("test migrations should run");
  sqlx::query("INSERT INTO users (user_id) VALUES (1)").execute(&pool).await.expect("test user should be inserted");
  pool
}

#[tokio::test]
async fn api_error_log_keeps_request_and_provider_details() {
  let pool = test_database().await;

  let mut server = mockito::Server::new_async().await;
  let upstream = server
    .mock("POST", "/chat/completions")
    .with_status(404)
    .with_header("content-type", "application/json")
    .with_body(r#"{"error":{"code":404,"message":"This model models/gemini-2.5-flash is no longer available to new users.","status":"NOT_FOUND"}}"#)
    .expect_at_least(1)
    .create_async()
    .await;

  let message = ChatCompletionRequestUserMessageArgs::default().content("private test prompt").build().expect("test message should build");
  let mut params = LlmRequestParams::new("models/gemini-2.5-flash", vec![message.into()], LlmRequestType::ApiPickDate);
  params.user_id = Some(1);

  let database = Database::Sqlite(pool.clone());
  let runtime = Runtime::new(&RuntimeConfig::default());
  let config = LlmClientConfig { api_key: "test".into(), api_base: server.url(), timeout_seconds: 5, http_client: None };

  assert!(call_llm(&database, &config, params, &runtime).await.is_err());
  runtime.tasks.close();
  runtime.tasks.wait().await;
  upstream.assert_async().await;

  let (request_type, is_success, request_body, response_body): (String, i64, String, String) =
    sqlx::query_as("SELECT request_type, is_success, request_body, response_body FROM llm_logs").fetch_one(&pool).await.expect("failed API call should be logged");
  assert_eq!(request_type, LlmRequestType::ApiPickDate.as_str());
  assert_eq!(is_success, 0);
  let request: serde_json::Value = serde_json::from_str(&request_body).expect("request log should be JSON");
  assert_eq!(request["model"], "models/gemini-2.5-flash");
  assert_eq!(request["messages"][0]["content"], "private test prompt");
  let response: serde_json::Value = serde_json::from_str(&response_body).expect("error log should be JSON");
  assert!(response["error"].as_str().expect("error text should exist").contains("no longer available to new users"));

  repos::Database::Sqlite(pool).close().await;
}

#[tokio::test]
async fn successful_stream_log_keeps_complete_text() {
  let pool = test_database().await;
  let mut server = mockito::Server::new_async().await;
  let upstream = server
    .mock("POST", "/chat/completions")
    .with_status(200)
    .with_header("content-type", "text/event-stream")
    .with_body("data: {\"id\":\"1\",\"object\":\"chat.completion.chunk\",\"created\":1694268190,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"First \"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"1\",\"object\":\"chat.completion.chunk\",\"created\":1694268190,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"second\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n")
    .expect(1)
    .create_async()
    .await;

  let message = ChatCompletionRequestUserMessageArgs::default().content("private stream prompt").build().expect("test message should build");
  let mut params = LlmRequestParams::new("gpt-4o", vec![message.into()], LlmRequestType::BaziAnalysis);
  params.user_id = Some(1);
  params.stream = Some(true);
  let database = Database::Sqlite(pool.clone());
  let runtime = Runtime::new(&RuntimeConfig::default());
  let config = LlmClientConfig { api_key: "test".into(), api_base: server.url(), timeout_seconds: 5, http_client: None };

  let baziflow_agent::models::LlmResponse::Stream(mut stream) = call_llm(&database, &config, params, &runtime).await.expect("stream should start") else {
    panic!("expected a stream");
  };
  let mut content = String::new();
  while let Some(chunk) = stream.recv().await {
    content.push_str(&chunk);
  }
  stream.finish().await.expect("stream should complete");
  assert_eq!(content, "First second");
  upstream.assert_async().await;

  let (request_body, response_body, is_success): (String, String, i64) =
    sqlx::query_as("SELECT request_body, response_body, is_success FROM llm_logs").fetch_one(&pool).await.expect("stream should be logged");
  let request: serde_json::Value = serde_json::from_str(&request_body).expect("request log should be JSON");
  assert_eq!(request["messages"][0]["content"], "private stream prompt");
  let response: serde_json::Value = serde_json::from_str(&response_body).expect("response log should be JSON");
  assert_eq!(response["content"], "First second");
  assert_eq!(is_success, 1);

  repos::Database::Sqlite(pool).close().await;
}

#[tokio::test]
async fn failed_stream_start_logs_provider_error() {
  let pool = test_database().await;
  let mut server = mockito::Server::new_async().await;
  let upstream = server
    .mock("POST", "/chat/completions")
    .with_status(404)
    .with_header("content-type", "application/json")
    .with_body(r#"{"error":{"code":404,"message":"This model models/gemini-2.5-flash is no longer available to new users.","status":"NOT_FOUND"}}"#)
    .expect_at_least(1)
    .create_async()
    .await;

  let message = ChatCompletionRequestUserMessageArgs::default().content("private Bazi chart").build().expect("test message should build");
  let mut params = LlmRequestParams::new("models/gemini-2.5-flash", vec![message.into()], LlmRequestType::BaziAnalysis);
  params.user_id = Some(1);
  params.stream = Some(true);
  let database = Database::Sqlite(pool.clone());
  let runtime = Runtime::new(&RuntimeConfig::default());
  let config = LlmClientConfig { api_key: "test".into(), api_base: server.url(), timeout_seconds: 5, http_client: None };

  assert!(call_llm(&database, &config, params, &runtime).await.is_err());
  upstream.assert_async().await;

  let (request_type, request_body, response_body, is_success): (String, String, String, i64) =
    sqlx::query_as("SELECT request_type, request_body, response_body, is_success FROM llm_logs").fetch_one(&pool).await.expect("failed stream should be logged");
  assert_eq!(request_type, LlmRequestType::BaziAnalysis.as_str());
  let request: serde_json::Value = serde_json::from_str(&request_body).expect("request log should be JSON");
  assert_eq!(request["messages"][0]["content"], "private Bazi chart");
  let response: serde_json::Value = serde_json::from_str(&response_body).expect("error log should be JSON");
  assert!(response["error"].as_str().expect("error text should exist").contains("no longer available to new users"));
  assert_eq!(is_success, 0);

  repos::Database::Sqlite(pool).close().await;
}

#[tokio::test]
async fn successful_full_response_is_logged() {
  let pool = test_database().await;
  let mut server = mockito::Server::new_async().await;
  let upstream = server
    .mock("POST", "/chat/completions")
    .with_status(200)
    .with_header("content-type", "application/json")
    .with_body(
      r#"{"id":"1","object":"chat.completion","created":1694268190,"model":"gpt-4o","choices":[{"index":0,"message":{"role":"assistant","content":"Full private reply"},"finish_reason":"stop"}]}"#,
    )
    .expect(1)
    .create_async()
    .await;

  let message = ChatCompletionRequestUserMessageArgs::default().content("private full prompt").build().expect("test message should build");
  let mut params = LlmRequestParams::new("gpt-4o", vec![message.into()], LlmRequestType::GenerateBaziSummary);
  params.user_id = Some(1);
  let database = Database::Sqlite(pool.clone());
  let runtime = Runtime::new(&RuntimeConfig::default());
  let config = LlmClientConfig { api_key: "test".into(), api_base: server.url(), timeout_seconds: 5, http_client: None };

  let baziflow_agent::models::LlmResponse::Full(content) = call_llm(&database, &config, params, &runtime).await.expect("request should succeed") else {
    panic!("expected a full response");
  };
  assert_eq!(content, "Full private reply");
  upstream.assert_async().await;

  let (request_body, response_body, is_success): (String, String, i64) =
    sqlx::query_as("SELECT request_body, response_body, is_success FROM llm_logs").fetch_one(&pool).await.expect("full response should be logged");
  let request: serde_json::Value = serde_json::from_str(&request_body).expect("request log should be JSON");
  assert_eq!(request["messages"][0]["content"], "private full prompt");
  let response: serde_json::Value = serde_json::from_str(&response_body).expect("response log should be JSON");
  assert_eq!(response["choices"][0]["message"]["content"], "Full private reply");
  assert_eq!(is_success, 1);

  repos::Database::Sqlite(pool).close().await;
}

#[tokio::test]
async fn interrupted_stream_log_keeps_partial_text_and_error() {
  let pool = test_database().await;
  let mut server = mockito::Server::new_async().await;
  let upstream = server
    .mock("POST", "/chat/completions")
    .with_status(200)
    .with_header("content-type", "text/event-stream")
    .with_body("data: {\"id\":\"1\",\"object\":\"chat.completion.chunk\",\"created\":1694268190,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Partial reply\"},\"finish_reason\":null}]}\n\ndata: {invalid-json}\n\n")
    .expect(1)
    .create_async()
    .await;

  let message = ChatCompletionRequestUserMessageArgs::default().content("private prompt").build().expect("test message should build");
  let mut params = LlmRequestParams::new("gpt-4o", vec![message.into()], LlmRequestType::BaziAnalysis);
  params.user_id = Some(1);
  params.stream = Some(true);
  let database = Database::Sqlite(pool.clone());
  let runtime = Runtime::new(&RuntimeConfig::default());
  let config = LlmClientConfig { api_key: "test".into(), api_base: server.url(), timeout_seconds: 5, http_client: None };

  let baziflow_agent::models::LlmResponse::Stream(mut stream) = call_llm(&database, &config, params, &runtime).await.expect("stream should start") else {
    panic!("expected a stream");
  };
  assert_eq!(stream.recv().await.as_deref(), Some("Partial reply"));
  assert!(stream.recv().await.is_none());
  assert!(stream.finish().await.is_err());
  upstream.assert_async().await;

  let (response_body, is_success): (String, i64) = sqlx::query_as("SELECT response_body, is_success FROM llm_logs").fetch_one(&pool).await.expect("interrupted stream should be logged");
  let response: serde_json::Value = serde_json::from_str(&response_body).expect("response log should be JSON");
  assert_eq!(response["content"], "Partial reply");
  assert!(response["error"].as_str().is_some_and(|error| error.contains("invalid-json")));
  assert_eq!(is_success, 0);

  repos::Database::Sqlite(pool).close().await;
}
