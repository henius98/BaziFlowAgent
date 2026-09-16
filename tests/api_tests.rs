mod test_helpers;
use axum::{
  body::Body,
  http::{Request, StatusCode, header},
};
use baziflow_agent::models::AppState;
use baziflow_agent::repos;
use mockito::ServerGuard;
use sqlx::SqlitePool;
use std::sync::Arc;
use tower::ServiceExt;

async fn setup_test_app() -> (axum::Router, ServerGuard, Arc<AppState>, String) {
  let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
  sqlx::migrate!().run(&pool).await.unwrap();

  let user_id = 1;
  sqlx::query("INSERT INTO users (user_id) VALUES (1)").execute(&pool).await.unwrap();

  let api_key = repos::create_api_key(&pool, user_id).await.unwrap();

  let server = mockito::Server::new_async().await;
  let mock_url = server.url();

  let config = Arc::new(test_helpers::test_config(mock_url));
  let state = test_helpers::test_state(config.clone(), pool.clone()).await;

  let _ = baziflow_agent::models::state::GLOBAL_STATE.set(state.clone());

  let app = baziflow_agent::api::api_router(config);
  (app, server, state, api_key)
}

#[tokio::test]
async fn test_all_api_endpoints() {
  let (app, mut server, _state, api_key) = setup_test_app().await;

  // ==========================================
  // 1. GET /profile (Not Found)
  // ==========================================
  let req = Request::builder().method("GET").uri("/api/v1/profile").header(header::AUTHORIZATION, format!("Bearer {}", api_key)).body(Body::empty()).unwrap();

  let response = app.clone().oneshot(req).await.unwrap();
  assert_eq!(response.status(), StatusCode::NOT_FOUND);

  // ==========================================
  // 2. POST /profile (Create Profile)
  // ==========================================
  let _m1 = server.mock("GET", mockito::Matcher::Regex(r"^/getbasebz8\.php.*".to_string())).with_status(200).with_body(include_str!("fixtures/chart.json")).create_async().await;
  let _m2 = server.mock("GET", mockito::Matcher::Regex(r"^/getRysl\.php.*".to_string())).with_status(200).with_body(r#"{"data":"some yongshi"}"#).create_async().await;
  let _m3 = server.mock("GET", mockito::Matcher::Regex(r"^/getGZRelaction3\.php.*".to_string())).with_status(200).with_body(r#"[["relation1"]]"#).create_async().await;
  let _m4 = server.mock("GET", mockito::Matcher::Regex(r"^/getliunianshensha5\.php.*".to_string())).with_status(200).with_body(r#"{"shensha":[[],["shensha1"]]}"#).create_async().await;

  let m5_sync = server.mock("POST", "/chat/completions")
        .match_body(mockito::Matcher::Regex(r#"(?s).*用户命盘详批.*"#.to_string()))
        .with_status(200)
        .with_body(r#"{"id":"1","object":"chat.completion","created":1694268190,"model":"gpt-4o","choices":[{"index":0,"message":{"role":"assistant","content":"基础：丁丑日；日时为主｜做功：子丑合，反局未载｜大运：未载｜性格：敏锐，易急｜应期：未载"},"finish_reason":"stop"}]}"#)
        .expect(1)
        .create_async().await;

  let m5_stream = server.mock("POST", "/chat/completions")
        .match_body(mockito::Matcher::Regex(r#"(?s).*待分析命盘.*"#.to_string()))
        .with_status(200)
        .with_header("content-type", "text/event-stream")
        .with_body("data: {\"id\":\"chatcmpl-123\",\"object\":\"chat.completion.chunk\",\"created\":1694268190,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Mocked Bazi Analysis\"},\"finish_reason\":null}]}\n\ndata: [DONE]\n\n")
        .expect(1)
        .create_async().await;

  let incomplete_time_json = serde_json::json!({
      "gender": 1,
      "birth_date": "1990-01-01",
      "birth_hour": 0
  })
  .to_string();

  let req = Request::builder()
    .method("POST")
    .uri("/api/v1/profile")
    .header(header::CONTENT_TYPE, "application/json")
    .header(header::AUTHORIZATION, format!("Bearer {}", api_key))
    .body(Body::from(incomplete_time_json))
    .unwrap();

  let response = app.clone().oneshot(req).await.unwrap();
  assert_eq!(response.status(), StatusCode::BAD_REQUEST);

  // Birth time and location are optional when both time fields are omitted.
  let body_json = serde_json::json!({
      "gender": 1,
      "birth_date": "1990-01-01"
  })
  .to_string();

  let req = Request::builder()
    .method("POST")
    .uri("/api/v1/profile")
    .header(header::CONTENT_TYPE, "application/json")
    .header(header::AUTHORIZATION, format!("Bearer {}", api_key))
    .body(Body::from(body_json))
    .unwrap();

  let response = app.clone().oneshot(req).await.unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  m5_stream.assert_async().await;
  m5_sync.assert_async().await;

  // ==========================================
  // 3. GET /profile (Found)
  // ==========================================
  let req = Request::builder().method("GET").uri("/api/v1/profile").header(header::AUTHORIZATION, format!("Bearer {}", api_key)).body(Body::empty()).unwrap();

  let response = app.clone().oneshot(req).await.unwrap();
  assert_eq!(response.status(), StatusCode::OK);

  // ==========================================
  // 4. PUT /model
  // ==========================================
  let req = Request::builder()
    .method("PUT")
    .uri("/api/v1/model")
    .header(header::CONTENT_TYPE, "application/json")
    .header(header::AUTHORIZATION, format!("Bearer {}", api_key))
    .body(Body::from(serde_json::json!({"model": 1}).to_string()))
    .unwrap();

  let response = app.clone().oneshot(req).await.unwrap();
  assert_eq!(response.status(), StatusCode::OK);

  // ==========================================
  // 5. PUT /schedule
  // ==========================================
  let req = Request::builder()
    .method("PUT")
    .uri("/api/v1/schedule")
    .header(header::CONTENT_TYPE, "application/json")
    .header(header::AUTHORIZATION, format!("Bearer {}", api_key))
    .body(Body::from(serde_json::json!({"time": "08:30"}).to_string()))
    .unwrap();

  let response = app.clone().oneshot(req).await.unwrap();
  assert_eq!(response.status(), StatusCode::OK);

  // ==========================================
  // 6. GET /date-fortune (WebSocket Upgrade)
  // ==========================================
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let addr = listener.local_addr().unwrap();
  let app_ws = app.clone();
  tokio::spawn(async move {
    let _ = axum::serve(baziflow_agent::api::listener(listener), app_ws).await;
  });

  let ws_url = format!("ws://{}/api/v1/date-fortune", addr);
  let mut request = tokio_tungstenite::tungstenite::client::IntoClientRequest::into_client_request(ws_url).unwrap();
  request.headers_mut().insert(header::AUTHORIZATION, header::HeaderValue::from_str(&format!("Bearer {}", api_key)).unwrap());

  let _m6 = server
    .mock("GET", mockito::Matcher::Regex(r"^/api/almanac.*".to_string()))
    .with_status(200)
    .with_body(r#"{"solar":{"year":2023,"month":10,"day":10},"ganZhi":{"year":"癸卯","month":"壬戌","day":"辛酉"},"yiJi":{"yi":["祈福"],"ji":["出行"]}}"#)
    .create_async()
    .await;

  let m7 = server.mock("POST", "/chat/completions")
        .match_body(mockito::Matcher::Regex(
            r#"(?s).*【目标预测日期】.*2023-10-10.*【该日黄历数据】.*"#.to_string(),
        ))
        .with_status(200)
        .with_header("content-type", "text/event-stream")
        .with_body("data: {\"id\":\"chatcmpl-123\",\"object\":\"chat.completion.chunk\",\"created\":1694268190,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Mocked Fortune WS\"},\"finish_reason\":null}]}\n\ndata: [DONE]\n\n")
        .expect(1)
        .create_async().await;

  let (mut ws_stream, response) = tokio_tungstenite::connect_async(request).await.expect("Failed to connect WS");
  assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);

  use futures::{SinkExt, StreamExt};
  use tokio_tungstenite::tungstenite::Message;

  ws_stream
    .send(Message::Text(
      serde_json::json!({
          "action": "generate",
          "date": "2023-10-10"
      })
      .to_string()
      .into(),
    ))
    .await
    .unwrap();

  // Read almanac
  let almanac_msg = ws_stream.next().await.unwrap().unwrap();
  let almanac_text = almanac_msg.to_text().unwrap();
  assert!(almanac_text.contains("almanac"));

  // Read full chunk response
  let chunk_msg = ws_stream.next().await.unwrap().unwrap();
  let chunk_text = chunk_msg.to_text().unwrap();
  assert!(chunk_text.contains("chunk"));

  // We can just close it now
  ws_stream.close(None).await.unwrap();
  m7.assert_async().await;

  // ==========================================
  // 7. POST /pick-date
  // ==========================================
  let m8 = server
    .mock("POST", "/chat/completions")
    .match_body(mockito::Matcher::Regex(r#"(?s).*【目标活动】.*"#.to_string()))
    .with_status(200)
    .with_body(r#"{"id":"1","object":"chat.completion","created":1694268190,"model":"gpt-4o","choices":[{"index":0,"message":{"role":"assistant","content":"Mocked Pick"},"finish_reason":"stop"}]}"#)
    .expect(1)
    .create_async()
    .await;

  let req = Request::builder()
    .method("POST")
    .uri("/api/v1/pick-date")
    .header(header::CONTENT_TYPE, "application/json")
    .header(header::AUTHORIZATION, format!("Bearer {}", api_key))
    .body(Body::from(
      serde_json::json!({
          "start_date": "2023-10-10",
          "end_date": "2023-10-15",
          "activity": "Wedding"
      })
      .to_string(),
    ))
    .unwrap();

  let response = app.clone().oneshot(req).await.unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  m8.assert_async().await;

  // ==========================================
  // 8. POST /chat
  // ==========================================
  let m9 = server
    .mock("POST", "/chat/completions")
    .match_body(mockito::Matcher::Regex(r#"(?s).*善于承接上下文的盲派八字与黄历问答助手.*应用提供的结构化命盘事实.*"#.to_string()))
    .with_status(200)
    .with_body(r#"{"id":"1","object":"chat.completion","created":1694268190,"model":"gpt-4o","choices":[{"index":0,"message":{"role":"assistant","content":"Mocked Reply"},"finish_reason":"stop"}]}"#)
    .expect(1)
    .create_async()
    .await;

  let req = Request::builder()
    .method("POST")
    .uri("/api/v1/chat")
    .header(header::CONTENT_TYPE, "application/json")
    .header(header::AUTHORIZATION, format!("Bearer {}", api_key))
    .body(Body::from(serde_json::json!({"message": "Hello"}).to_string()))
    .unwrap();

  let response = app.clone().oneshot(req).await.unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  m9.assert_async().await;

  // ==========================================
  // 9. POST /chat?stream=true
  // ==========================================
  let m10 = server.mock("POST", "/chat/completions")
        .match_body(mockito::Matcher::Regex(
            r#"(?s).*善于承接上下文的盲派八字与黄历问答助手.*应用提供的结构化命盘事实.*"#.to_string(),
        ))
        .with_status(200)
        .with_header("content-type", "text/event-stream")
        .with_body("data: {\"id\":\"chatcmpl-123\",\"object\":\"chat.completion.chunk\",\"created\":1694268190,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Streaming chat reply\"},\"finish_reason\":null}]}\n\ndata: [DONE]\n\n")
        .expect(1)
        .create_async().await;

  let req = Request::builder()
    .method("POST")
    .uri("/api/v1/chat?stream=true")
    .header(header::CONTENT_TYPE, "application/json")
    .header(header::AUTHORIZATION, format!("Bearer {}", api_key))
    .body(Body::from(serde_json::json!({"message": "Hello stream"}).to_string()))
    .unwrap();

  let response = app.clone().oneshot(req).await.unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "text/event-stream");

  let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
  let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();
  assert!(body_str.contains("Streaming chat reply"));
  m10.assert_async().await;
}
