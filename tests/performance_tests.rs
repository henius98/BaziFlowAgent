mod test_helpers;
use baziflow_agent::{api, models::state::GLOBAL_STATE, repos, services::paipan::models::RawBaziChart};
use futures::{SinkExt, StreamExt};
use std::{hint::black_box, sync::Arc, time::Instant};
use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest};

const CHART: &str = include_str!("fixtures/chart.json");

#[test]
#[ignore = "repeatable parser microbenchmark; run serially in release mode"]
fn chart_parsing_before_after() {
  for run in 0..5 {
    let start = Instant::now();
    for _ in 0..20_000 {
      let value: serde_json::Value = serde_json::from_str(black_box(CHART)).unwrap();
      black_box(serde_json::from_value::<RawBaziChart>(value).unwrap());
    }
    let before = start.elapsed().as_micros();
    let start = Instant::now();
    for _ in 0..20_000 {
      black_box(serde_json::from_str::<RawBaziChart>(black_box(CHART)).unwrap());
    }
    println!("parse run={run} iterations=20000 before_us={before} after_us={}", start.elapsed().as_micros());
  }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "loopback WebSocket load test; no external services contacted"]
async fn websocket_load() {
  let pool = repos::init_db("sqlite::memory:").await.unwrap();
  let database = repos::Database::Sqlite(pool.clone());
  let mut config = test_helpers::test_config("http://127.0.0.1:1".into());
  config.runtime.max_connections = 1200;
  let config = Arc::new(config);
  let state = test_helpers::test_state(config.clone(), pool.clone()).await;
  GLOBAL_STATE.set(state.clone()).unwrap();
  let app = api::api_router(config);
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let url = format!("ws://{}/api/v1/ws", listener.local_addr().unwrap());
  let shutdown = state.runtime.shutdown.clone();
  let server = tokio::spawn(async move {
    axum::serve(baziflow_agent::api::listener(listener), app).with_graceful_shutdown(shutdown.cancelled_owned()).await.unwrap();
  });
  let mut offset = 0;
  for count in [1, 10, 100, 1000] {
    let mut credentials = Vec::new();
    for id in offset + 1..=offset + count {
      sqlx::query("INSERT INTO users(user_id) VALUES(?)").bind(id).execute(&pool).await.unwrap();
      credentials.push(repos::create_api_key(&database, id as u64).await.unwrap());
    }
    offset += count;
    let start = Instant::now();
    let sockets = futures::stream::iter(credentials)
      .map(|key| {
        let url = url.clone();
        async move {
          let mut request = url.into_client_request().unwrap();
          request.headers_mut().insert("authorization", format!("Bearer {key}").parse().unwrap());
          tokio_tungstenite::connect_async(request).await.unwrap().0
        }
      })
      .buffer_unordered(16)
      .collect::<Vec<_>>()
      .await;
    let connect_ms = start.elapsed().as_millis();
    let rss_kib = std::fs::read_to_string("/proc/self/status").unwrap().lines().find(|line| line.starts_with("VmRSS:")).unwrap().to_owned();
    let start = Instant::now();
    let samples = futures::stream::iter(sockets)
      .map(|mut socket| async move {
        let mut times = Vec::new();
        for id in 0..20 {
          let start = Instant::now();
          socket.send(Message::Text(format!(r#"{{"version":1,"id":"r{id}","action":{{"name":"get_state"}}}}"#).into())).await.unwrap();
          loop {
            match socket.next().await.unwrap().unwrap() {
              Message::Ping(bytes) => socket.send(Message::Pong(bytes)).await.unwrap(),
              Message::Text(text) => {
                let response: serde_json::Value = serde_json::from_str(&text).unwrap();
                assert_eq!(response["ok"], true);
                assert_eq!(response["id"], format!("r{id}"));
                break;
              }
              other => panic!("unexpected frame {other:?}"),
            }
          }
          times.push(start.elapsed().as_micros());
        }
        socket.close(None).await.unwrap();
        times
      })
      .buffer_unordered(count as usize)
      .collect::<Vec<_>>()
      .await;
    let elapsed = start.elapsed();
    let mut samples: Vec<_> = samples.into_iter().flatten().collect();
    samples.sort_unstable();
    println!(
      "ws clients={count} requests={} connect_ms={connect_ms} throughput_rps={:.1} p50_us={} p95_us={} p99_us={} rss={rss_kib}",
      samples.len(),
      samples.len() as f64 / elapsed.as_secs_f64(),
      samples[samples.len() / 2],
      samples[samples.len() * 95 / 100],
      samples[samples.len() * 99 / 100]
    );
  }
  state.runtime.shutdown.cancel();
  tokio::time::timeout(std::time::Duration::from_secs(10), server).await.unwrap().unwrap();
  state.runtime.tasks.close();
  tokio::time::timeout(std::time::Duration::from_secs(10), state.runtime.tasks.wait()).await.unwrap();
  assert_eq!(state.runtime.connections.available_permits(), 1200);
}
