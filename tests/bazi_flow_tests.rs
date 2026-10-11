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
  let hour_pillar = structured_data.pillars.iter().find(|pillar| pillar.name == "时柱").expect("hour pillar must exist");
  assert_eq!(hour_pillar.base.stem_and_stars, vec![("*".to_string(), "*".to_string())]);
  assert_eq!(hour_pillar.base.branch, "*");
  assert_eq!(hour_pillar.base.hidden_stems_and_stars, vec![("*".to_string(), "*".to_string())]);
  assert_eq!(hour_pillar.base.star_luck, "*");
  assert_eq!(hour_pillar.base.self_sitting, "*");
  assert_eq!(hour_pillar.base.empty_death, "*");
  assert_eq!(hour_pillar.base.nayin, "*");
  assert_eq!(hour_pillar.base.shensha, vec!["*".to_string()]);

  let receiver = core_bazi_analysis(&state, 123_u64, &structured_data, None::<baziflow_agent::models::common::LlmModel>).await.expect("Failed to run core analysis");
  let mut rx = receiver;
  let mut out = String::new();
  while let Some(chunk) = rx.recv().await {
    out.push_str(std::str::from_utf8(&chunk).unwrap());
  }
  rx.finish().await.unwrap();
  assert_eq!(out, "Hello World");

  let failed = server.mock("POST", "/chat/completions").with_status(200).with_header("content-type", "text/event-stream").with_body("data: {invalid-json}\n\n").create_async().await;
  let mut broken = core_bazi_analysis(&state, 123, &structured_data, None).await.unwrap();
  while broken.recv().await.is_some() {}
  assert!(broken.finish().await.is_err());
  failed.assert_async().await;
}

#[tokio::test]
async fn test_create_destiny_profile() {
  let pool = SqlitePool::connect("sqlite::memory:").await.expect("Failed to connect to memory db");
  sqlx::migrate!().run(&pool).await.expect("Failed to run migrations");

  let mut server = Server::new_async().await;
  let mock_url = server.url();

  server.mock("GET", mockito::Matcher::Regex(r"^/getbasebz8\.php.*".to_string())).with_status(200).with_body(include_str!("fixtures/chart.json")).create_async().await;
  server.mock("GET", mockito::Matcher::Regex(r"^/getRysl\.php.*".to_string())).with_status(200).with_body(r#"{"data":"some yongshi"}"#).create_async().await;
  server.mock("GET", mockito::Matcher::Regex(r"^/getGZRelaction3\.php.*".to_string())).with_status(200).with_body(r#"[["relation1"]]"#).create_async().await;
  server.mock("GET", mockito::Matcher::Regex(r"^/getliunianshensha5\.php.*".to_string())).with_status(200).with_body(r#"{"shensha":[[],["shensha1"]]}"#).create_async().await;

  server
    .mock("POST", "/chat/completions")
    .match_body(mockito::Matcher::Regex(r#"(?s).*待分析命盘.*"#.to_string()))
    .with_status(200)
    .with_header("content-type", "text/event-stream")
    .with_body("data: {\"id\":\"chatcmpl-123\",\"object\":\"chat.completion.chunk\",\"created\":1694268190,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Destiny Reading Generated\"},\"finish_reason\":null}]}\n\ndata: [DONE]\n\n")
    .create_async()
    .await;

  server
    .mock("POST", "/chat/completions")
    .match_body(mockito::Matcher::Regex(r#"(?s).*用户命盘详批.*"#.to_string()))
    .with_status(200)
    .with_body(r#"{"id":"1","object":"chat.completion","created":1694268190,"model":"gpt-4o","choices":[{"index":0,"message":{"role":"assistant","content":"基础：丁丑日;日时为主｜做功：子丑合，反局未载｜大运：未载｜性格：敏锐，易急｜应期：未载"},"finish_reason":"stop"}]}"#)
    .create_async()
    .await;

  let config = std::sync::Arc::new(test_helpers::test_config(mock_url));
  let state = test_helpers::test_state(config, pool.clone()).await;
  let _ = tokio::fs::create_dir_all("public").await;

  let params = DestinyProfileParams { user_id: 456, username: "DestinyUser", birth_date: "1995-05-20", birth_time: None, gender: 0, location: Some("Beijing".to_string()), llm_model: None };

  let profile_res = create_destiny_profile(&state, params).await.expect("create_destiny_profile succeeds");
  assert!(profile_res.chart_url.contains("456") || profile_res.chart_url.contains("charts/"));

  let mut rx = profile_res.stream;
  let mut text = String::new();
  while let Some(chunk) = rx.recv().await {
    text.push_str(std::str::from_utf8(&chunk).unwrap());
  }
  rx.finish().await.expect("stream finishes successfully");
  assert_eq!(text, "Destiny Reading Generated");

  let profile = baziflow_agent::repos::get_user_profile(&state.db_pool, 456).await;
  assert_eq!(profile.bazi_analysis.as_deref(), Some("Destiny Reading Generated"));
  assert_eq!(profile.bazi_summary.as_deref(), Some("基础：丁丑日;日时为主｜做功：子丑合，反局未载｜大运：未载｜性格：敏锐，易急｜应期：未载"));
}

#[tokio::test]
async fn test_submit_chat_turn() {
  let pool = SqlitePool::connect("sqlite::memory:").await.expect("Failed to connect to memory db");
  sqlx::migrate!().run(&pool).await.expect("Failed to run migrations");

  let mut server = Server::new_async().await;
  let mock_url = server.url();

  let config = std::sync::Arc::new(test_helpers::test_config(mock_url));
  let state = test_helpers::test_state(config, pool.clone()).await;

  // 1. Missing profile returns ProfileNotFound
  let err = baziflow_agent::services::conversation::submit_chat_turn(&state, 789, "Will I be rich?", true, baziflow_agent::models::LlmRequestType::FollowUpChat, None).await;
  assert!(matches!(err, Err(baziflow_agent::models::AppError::ProfileNotFound)));

  // 2. Setup user profile
  server.mock("GET", mockito::Matcher::Regex(r"^/getbasebz8\.php.*".to_string())).with_status(200).with_body(include_str!("fixtures/chart.json")).create_async().await;
  server.mock("GET", mockito::Matcher::Regex(r"^/getRysl\.php.*".to_string())).with_status(200).with_body(r#"{"data":"some yongshi"}"#).create_async().await;
  server.mock("GET", mockito::Matcher::Regex(r"^/getGZRelaction3\.php.*".to_string())).with_status(200).with_body(r#"[["relation1"]]"#).create_async().await;
  server.mock("GET", mockito::Matcher::Regex(r"^/getliunianshensha5\.php.*".to_string())).with_status(200).with_body(r#"{"shensha":[[],["shensha1"]]}"#).create_async().await;

  let bazi_params = BaziDataParams { user_id: 789, username: "ChatUser", birth_date: "1990-01-01", birth_time: None, gender: 1, location: None };
  let structured_data = prepare_bazi_data(&state, bazi_params).await.expect("prepare_bazi_data succeeds");
  let chart_json = serde_json::to_string(&structured_data).expect("serialize structured data");
  baziflow_agent::repos::upsert_user_bazi(&state.db_pool, 789, Some("ChatUser"), &chart_json, 1, "1990-01-01").await.unwrap();

  server
    .mock("POST", "/chat/completions")
    .match_body(mockito::Matcher::Regex(r#"(?s).*善于承接上下文的盲派八字与黄历问答助手.*"#.to_string()))
    .with_status(200)
    .with_header("content-type", "text/event-stream")
    .with_body("data: {\"id\":\"chatcmpl-456\",\"object\":\"chat.completion.chunk\",\"created\":1694268190,\"model\":\"gpt-4o\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Yes, with diligence.\"},\"finish_reason\":null}]}\n\ndata: [DONE]\n\n")
    .create_async()
    .await;

  let response = baziflow_agent::services::conversation::submit_chat_turn(&state, 789, "Will I be rich?", true, baziflow_agent::models::LlmRequestType::FollowUpChat, None)
    .await
    .expect("submit_chat_turn succeeds");

  match response {
    baziflow_agent::models::LlmResponse::Stream(mut rx) => {
      let mut reply = String::new();
      while let Some(chunk) = rx.recv().await {
        reply.push_str(std::str::from_utf8(&chunk).unwrap());
      }
      rx.finish().await.expect("chat stream completes");
      assert_eq!(reply, "Yes, with diligence.");
    }
    _ => panic!("Expected Stream response"),
  }

  // Verify memory context recorded both User and Assistant turns
  let ctx = state.user_contexts.get(&789).expect("user context exists");
  assert_eq!(ctx.messages.len(), 2);
  assert_eq!(ctx.messages[0].role, baziflow_agent::models::ChatRole::User);
  assert_eq!(&*ctx.messages[0].content, "Will I be rich?");
  assert_eq!(ctx.messages[1].role, baziflow_agent::models::ChatRole::Assistant);
  assert_eq!(&*ctx.messages[1].content, "Yes, with diligence.");
}
