mod test_helpers;
use axum::{
  body::Body,
  http::{Request, StatusCode, header},
};
use baziflow_agent::{api, models::state::GLOBAL_STATE, repos};
use futures::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio_tungstenite::{
  MaybeTlsStream, WebSocketStream,
  tungstenite::{Message, client::IntoClientRequest},
};
use tower::ServiceExt;
type Socket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

async fn connect(url: &str, key: &str) -> Socket {
  let mut request = url.into_client_request().unwrap();
  request.headers_mut().insert(header::AUTHORIZATION, format!("Bearer {key}").parse().unwrap());
  tokio_tungstenite::connect_async(request).await.unwrap().0
}

async fn receive(socket: &mut Socket) -> Value {
  tokio::time::timeout(Duration::from_secs(5), async {
    loop {
      match socket.next().await.unwrap().unwrap() {
        Message::Text(text) => return serde_json::from_str(&text).unwrap(),
        Message::Ping(data) => socket.send(Message::Pong(data)).await.unwrap(),
        other => panic!("Unexpected frame: {other:?}"),
      }
    }
  })
  .await
  .unwrap()
}
async fn request(socket: &mut Socket, id: &str, action: Value) -> Value {
  socket.send(Message::Text(json!({"version":1,"id":id,"action":action}).to_string().into())).await.unwrap();
  receive(socket).await
}

#[tokio::test]
async fn integration_boundary_lifecycle_and_authorization() {
  let pool = repos::init_db("sqlite::memory:").await.unwrap();
  let database = repos::Database::Sqlite(pool.clone());
  for user in 1..=4 {
    sqlx::query("INSERT INTO users(user_id) VALUES(?)").bind(user).execute(&pool).await.unwrap();
  }
  let key = repos::create_api_key(&database, 1).await.unwrap();
  let key2 = repos::create_api_key(&database, 2).await.unwrap();
  let mut config = test_helpers::test_config("http://127.0.0.1:1".into());
  config.runtime.max_connections = 4;
  config.runtime.idle_seconds = 3;
  config.cors_allowed_origin = "https://allowed.example".into();
  let config = Arc::new(config);
  let state = test_helpers::test_state(config.clone(), pool.clone()).await;
  GLOBAL_STATE.set(state.clone()).unwrap();
  let guard = baziflow_agent::models::ProcessingGuard::acquire(state.clone(), 4).unwrap();
  assert!(baziflow_agent::models::ProcessingGuard::acquire(state.clone(), 4).is_none());
  let (ready, started) = tokio::sync::oneshot::channel();
  let task = tokio::spawn(async move {
    let _guard = guard;
    ready.send(()).unwrap();
    std::future::pending::<()>().await;
  });
  started.await.unwrap();
  task.abort();
  assert!(task.await.unwrap_err().is_cancelled());
  assert!(!state.user_contexts.get(&4).unwrap().is_processing);
  assert_eq!(state.runtime.work.available_permits(), state.config.runtime.max_work);
  let app = api::api_router(config);
  let response = app.clone().oneshot(Request::builder().uri("/api/v1/profile").body(Body::empty()).unwrap()).await.unwrap();
  assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
  let response = app.clone().oneshot(Request::builder().uri(format!("/api/v1/ws?token={key}")).body(Body::empty()).unwrap()).await.unwrap();
  assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let url = format!("ws://{}/api/v1/ws", listener.local_addr().unwrap());
  let shutdown = state.runtime.shutdown.clone();
  let server = tokio::spawn(async move {
    axum::serve(baziflow_agent::api::listener(listener), app).with_graceful_shutdown(shutdown.cancelled_owned()).await.unwrap();
  });
  let mut bad_origin = url.as_str().into_client_request().unwrap();
  bad_origin.headers_mut().insert(header::AUTHORIZATION, format!("Bearer {key}").parse().unwrap());
  bad_origin.headers_mut().insert(header::ORIGIN, "https://attacker.example".parse().unwrap());
  assert!(matches!(tokio_tungstenite::connect_async(bad_origin).await, Err(tokio_tungstenite::tungstenite::Error::Http(response)) if response.status() == StatusCode::FORBIDDEN));

  let mut a = connect(&url, &key).await;
  let mut b = connect(&url, &key2).await;
  let response = request(&mut a, "subscribe-1", json!({"name":"subscribe","event":"profile.updated"})).await;
  assert_eq!(response["id"], "subscribe-1");
  assert_eq!(request(&mut a, "bad-event", json!({"name":"subscribe","event":"telegram.raw"})).await["error"]["code"], "subscription_not_allowed");
  let response = request(&mut a, "forged", json!({"name":"get_state","user_id":2})).await;
  assert_eq!(response["error"]["code"], "invalid_request");
  assert_eq!(request(&mut a, "set-1", json!({"name":"set_model","model":1})).await["ok"], true);
  assert_eq!(receive(&mut a).await["event"], "profile.updated");
  assert!(request(&mut b, "read-other", json!({"name":"get_state"})).await["data"]["model"].is_null());
  assert_eq!(request(&mut a, "unsubscribe", json!({"name":"unsubscribe"})).await["ok"], true);
  assert_eq!(request(&mut a, "set-1", json!({"name":"set_model","model":1})).await["ok"], true);
  assert_eq!(request(&mut a, "read-own", json!({"name":"get_state"})).await["data"]["model"], 1);
  let mut second = connect(&url, &key).await;
  let mut third = url.as_str().into_client_request().unwrap();
  third.headers_mut().insert(header::AUTHORIZATION, format!("Bearer {key}").parse().unwrap());
  assert!(matches!(tokio_tungstenite::connect_async(third).await, Err(tokio_tungstenite::tungstenite::Error::Http(response)) if response.status() == StatusCode::TOO_MANY_REQUESTS));
  second.close(None).await.unwrap();

  let _rotated = repos::create_api_key(&database, 1).await.unwrap();
  a.send(Message::Text(json!({"version":1,"id":"revoked","action":{"name":"get_state"}}).to_string().into())).await.unwrap();
  tokio::time::timeout(Duration::from_secs(5), async {
    loop {
      match a.next().await {
        Some(Ok(Message::Close(frame))) => {
          assert_eq!(frame.unwrap().reason, "credential_revoked");
          break;
        }
        Some(Ok(Message::Ping(data))) => {
          let _ = a.send(Message::Pong(data)).await;
        }
        other => panic!("{other:?}"),
      }
    }
  })
  .await
  .unwrap();

  let _ = a.flush().await;

  b.send(Message::Text("x".repeat(100_000).into())).await.unwrap();
  tokio::time::timeout(Duration::from_secs(5), async {
    loop {
      match b.next().await {
        Some(Ok(Message::Ping(data))) => {
          let _ = b.send(Message::Pong(data)).await;
        }
        Some(Ok(Message::Text(_))) => panic!("oversized frame accepted"),
        _ => break,
      }
    }
  })
  .await
  .unwrap();
  let key3 = repos::create_api_key(&database, 3).await.unwrap();
  let mut idle = connect(&url, &key3).await;
  tokio::time::sleep(Duration::from_secs(4)).await;
  tokio::time::timeout(Duration::from_secs(2), async {
    loop {
      match idle.next().await {
        Some(Ok(Message::Close(frame))) => {
          assert_eq!(frame.unwrap().reason, "heartbeat_timeout");
          break;
        }
        Some(Ok(_)) => {}
        other => panic!("{other:?}"),
      }
    }
  })
  .await
  .unwrap();

  let _ = idle.flush().await;
  let key4 = repos::create_api_key(&database, 4).await.unwrap();
  let mut final_socket = connect(&url, &key4).await;
  state.runtime.shutdown.cancel();
  tokio::time::timeout(Duration::from_secs(5), async {
    loop {
      match final_socket.next().await {
        Some(Ok(Message::Close(frame))) => {
          assert_eq!(frame.unwrap().reason, "server_shutdown");
          break;
        }
        Some(Ok(_)) => {}
        other => panic!("{other:?}"),
      }
    }
  })
  .await
  .unwrap();
  let _ = final_socket.flush().await;
  tokio::time::timeout(Duration::from_secs(5), server).await.unwrap().unwrap();
  state.runtime.tasks.close();
  tokio::time::timeout(Duration::from_secs(6), state.runtime.tasks.wait()).await.unwrap();
  assert_eq!(state.runtime.connections.available_permits(), 4);
  state.chat_cache_writer.flush().await.unwrap();
}
