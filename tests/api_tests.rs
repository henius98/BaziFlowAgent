use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use baziflow_agent::config::AppConfig;
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
    sqlx::query("INSERT INTO users (user_id) VALUES (1)")
        .execute(&pool)
        .await
        .unwrap();

    let api_key = repos::create_api_key(&pool, user_id).await.unwrap();

    let server = mockito::Server::new_async().await;
    let mock_url = server.url();

    let config = AppConfig {
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
        app_timezone: chrono_tz::Tz::UTC,
        log_retention_days: 7,
        max_context_messages: 10,
        base_url: "http://localhost".into(),
        log_level: "info".into(),
        r2_account_id: None,
        r2_access_key_id: None,
        r2_secret_access_key: None,
        r2_bucket_name: None,
    };

    let state = Arc::new(AppState::new(
        reqwest::Client::new(),
        pool.clone(),
        Arc::new(config),
    ));
    let _ = baziflow_agent::models::state::GLOBAL_STATE.set(state.clone());

    let app = baziflow_agent::api::api_router();
    (app, server, state, api_key)
}

#[tokio::test]
async fn test_all_api_endpoints() {
    let (app, mut server, _state, api_key) = setup_test_app().await;

    // ==========================================
    // 1. GET /profile (Not Found)
    // ==========================================
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/profile")
        .header(header::AUTHORIZATION, format!("Bearer {}", api_key))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // ==========================================
    // 2. POST /profile (Create Profile)
    // ==========================================
    let _m1 = server.mock("GET", mockito::Matcher::Regex(r"^/getbasebz8\.php.*".to_string()))
        .with_status(200)
        .with_body(r#"{"info":{"gender":"男,乾造","solar_date":"1990-01-01 00:00:00","lunisolar_date":"一九八九年十二月初五日子时"},"bz":{"year_steam":"己","year_branch":"巳","month_steam":"丙","month_branch":"子","day_steam":"丁","day_branch":"丑","hour_steam":"庚","hour_branch":"子"},"dyshensha":[],"lnshensha":[]}"#)
        .create_async().await;
    let _m2 = server
        .mock(
            "GET",
            mockito::Matcher::Regex(r"^/getRysl\.php.*".to_string()),
        )
        .with_status(200)
        .with_body(r#"{"data":"some yongshi"}"#)
        .create_async()
        .await;
    let _m3 = server
        .mock(
            "GET",
            mockito::Matcher::Regex(r"^/getGZRelaction3\.php.*".to_string()),
        )
        .with_status(200)
        .with_body(r#"[["relation1"]]"#)
        .create_async()
        .await;
    let _m4 = server
        .mock(
            "GET",
            mockito::Matcher::Regex(r"^/getliunianshensha5\.php.*".to_string()),
        )
        .with_status(200)
        .with_body(r#"{"shensha":[[],["shensha1"]]}"#)
        .create_async()
        .await;

    let m5_sync = server.mock("POST", "/chat/completions")
        .match_body(mockito::Matcher::Regex(r#"(?s).*用户命盘详批.*"#.to_string()))
        .with_status(200)
        .with_body(r#"{"id":"1","object":"chat.completion","created":1694268190,"model":"gpt-4o","choices":[{"index":0,"message":{"role":"assistant","content":"Mocked Summary"},"finish_reason":"stop"}]}"#)
        .expect(1)
        .create_async().await;

    let m5_stream = server.mock("POST", "/chat/completions")
        .match_body(mockito::Matcher::Regex(r#"(?s).*待分析命盘.*"#.to_string()))
        .with_status(200)
        .with_header("content-type", "text/event-stream")
        .with_body("data: {\"id\":\"chatcmpl-123\",\"object\":\"chat.completion.chunk\",\"created\":1694268190,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Mocked Bazi Analysis\"},\"finish_reason\":null}]}\n\ndata: [DONE]\n\n")
        .expect(1)
        .create_async().await;

    let body_json = serde_json::json!({
        "gender": 1,
        "birth_date": "1990-01-01",
        "birth_hour": 0,
        "birth_minute": 0,
        "location": "Beijing"
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
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/profile")
        .header(header::AUTHORIZATION, format!("Bearer {}", api_key))
        .body(Body::empty())
        .unwrap();

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
    // 6. POST /date-fortune
    // ==========================================
    let _m6 = server.mock("GET", mockito::Matcher::Regex(r"^/getHuangli\.php.*".to_string()))
        .with_status(200)
        .with_body(r#"{"y":"2023","m":"10","d":"10","nongli":"八月廿六","suici":["癸卯年","壬戌月","辛酉日"],"yi":["祈福"],"ji":["出行"],"jsyq":["天恩"],"xsyq":["死神"],"wuxing":"石榴木","chong":"兔","sha":"东","jsyq_desc":["..."],"xsyq_desc":["..."],"pengzujiji":["辛不合酱","酉不会客"],"caishen":"正东","xishen":"西南","fushen":"西南","js_list":[{"time":"0:00-0:59","dz":"子","jx":"凶"}],"sc_list":[{"time":"0:00-0:59","dz":"子","sc":"...","cx":"..."}]}"#)
        .create_async().await;

    let m7 = server.mock("POST", "/chat/completions")
        .match_body(mockito::Matcher::Regex(r#"(?s).*【目标预测日期】.*"#.to_string()))
        .with_status(200)
        .with_body(r#"{"id":"1","object":"chat.completion","created":1694268190,"model":"gpt-4o","choices":[{"index":0,"message":{"role":"assistant","content":"Mocked Fortune"},"finish_reason":"stop"}]}"#)
        .expect(1)
        .create_async().await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/date-fortune")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {}", api_key))
        .body(Body::from(
            serde_json::json!({"date": "2023-10-10"}).to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    m7.assert_async().await;

    // ==========================================
    // 7. POST /pick-date
    // ==========================================
    let m8 = server.mock("POST", "/chat/completions")
        .match_body(mockito::Matcher::Regex(r#"(?s).*【目标活动】.*"#.to_string()))
        .with_status(200)
        .with_body(r#"{"id":"1","object":"chat.completion","created":1694268190,"model":"gpt-4o","choices":[{"index":0,"message":{"role":"assistant","content":"Mocked Pick"},"finish_reason":"stop"}]}"#)
        .expect(1)
        .create_async().await;

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
    let m9 = server.mock("POST", "/chat/completions")
        .match_body(mockito::Matcher::Regex(r#"(?s).*You are a professional AI Bazi.*"#.to_string()))
        .with_status(200)
        .with_body(r#"{"id":"1","object":"chat.completion","created":1694268190,"model":"gpt-4o","choices":[{"index":0,"message":{"role":"assistant","content":"Mocked Reply"},"finish_reason":"stop"}]}"#)
        .expect(1)
        .create_async().await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/chat")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {}", api_key))
        .body(Body::from(
            serde_json::json!({"message": "Hello"}).to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    m9.assert_async().await;

    // ==========================================
    // 9. POST /chat?stream=true
    // ==========================================
    let m10 = server.mock("POST", "/chat/completions")
        .match_body(mockito::Matcher::Regex(r#"(?s).*You are a professional AI Bazi.*"#.to_string()))
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
        .body(Body::from(
            serde_json::json!({"message": "Hello stream"}).to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/event-stream"
    );

    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();
    assert!(body_str.contains("Streaming chat reply"));
    m10.assert_async().await;
}
