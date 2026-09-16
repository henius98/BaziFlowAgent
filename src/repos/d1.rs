use crate::{
  config::D1Config,
  models::{AppError, AppResult},
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;

const D1_API_BASE: &str = "https://api.cloudflare.com/client/v4";

#[derive(Clone)]
pub struct D1Database {
  client: Client,
  query_url: String,
  api_token: String,
}

impl std::fmt::Debug for D1Database {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("D1Database").field("query_url", &self.query_url).finish_non_exhaustive()
  }
}

#[derive(Serialize)]
struct D1Statement<'a> {
  sql: &'a str,
  #[serde(skip_serializing_if = "Vec::is_empty")]
  params: Vec<Value>,
}

#[derive(Serialize)]
struct D1Batch<'a> {
  batch: Vec<D1Statement<'a>>,
}

#[derive(Deserialize)]
struct D1Response {
  success: bool,
  #[serde(default)]
  result: Vec<D1QueryResult>,
  #[serde(default)]
  errors: Vec<D1ApiMessage>,
}

#[derive(Deserialize)]
struct D1QueryResult {
  #[serde(default)]
  success: bool,
  #[serde(default)]
  results: Vec<Map<String, Value>>,
}

#[derive(Deserialize)]
struct D1ApiMessage {
  code: Option<u64>,
  message: String,
}

impl D1Database {
  pub async fn connect(client: Client, config: &D1Config) -> AppResult<Self> {
    let database = Self::new(client, config, D1_API_BASE);
    database.apply_migrations().await?;
    Ok(database)
  }

  fn new(client: Client, config: &D1Config, api_base: &str) -> Self {
    Self { client, query_url: format!("{}/accounts/{}/d1/database/{}/query", api_base.trim_end_matches('/'), config.account_id, config.database_id), api_token: config.api_token.clone() }
  }

  pub async fn execute(&self, sql: &str, params: Vec<Value>) -> AppResult<()> {
    self.send(&D1Statement { sql, params }).await.map(|_| ())
  }

  pub async fn fetch_all(&self, sql: &str, params: Vec<Value>) -> AppResult<Vec<Map<String, Value>>> {
    let mut result = self.send(&D1Statement { sql, params }).await?;
    Ok(result.pop().map(|result| result.results).unwrap_or_default())
  }

  pub async fn batch(&self, statements: Vec<(&str, Vec<Value>)>) -> AppResult<()> {
    let batch = D1Batch { batch: statements.into_iter().map(|(sql, params)| D1Statement { sql, params }).collect() };
    self.send(&batch).await.map(|_| ())
  }

  async fn send<T: Serialize + ?Sized>(&self, body: &T) -> AppResult<Vec<D1QueryResult>> {
    let response = self.client.post(&self.query_url).bearer_auth(&self.api_token).json(body).send().await?;
    let status = response.status();
    let response_body = response.text().await?;
    let payload: D1Response = serde_json::from_str(&response_body).map_err(|error| AppError::D1(format!("Cloudflare returned HTTP {status} with an invalid response: {error}")))?;

    if !status.is_success() || !payload.success || payload.result.iter().any(|result| !result.success) {
      let details = payload
        .errors
        .into_iter()
        .map(|error| match error.code {
          Some(code) => format!("{code}: {}", error.message),
          None => error.message,
        })
        .collect::<Vec<_>>()
        .join(", ");
      let details = if details.is_empty() { "query failed without error details" } else { &details };
      return Err(AppError::D1(format!("Cloudflare returned HTTP {status}: {details}")));
    }

    Ok(payload.result)
  }

  async fn apply_migrations(&self) -> AppResult<()> {
    self
      .execute(
        "CREATE TABLE IF NOT EXISTS _baziflow_migrations (version INTEGER PRIMARY KEY, checksum TEXT NOT NULL, applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S', 'now')))",
        Vec::new(),
      )
      .await?;

    let applied = self
      .fetch_all("SELECT version, checksum FROM _baziflow_migrations", Vec::new())
      .await?
      .into_iter()
      .filter_map(|row| Some((json_i64(&row, "version")?, row.get("checksum")?.as_str()?.to_owned())))
      .collect::<HashMap<_, _>>();

    for migration in crate::repos::MIGRATOR.iter().filter(|migration| !migration.migration_type.is_down_migration()) {
      let checksum = hex::encode(migration.checksum.as_ref());
      if let Some(applied_checksum) = applied.get(&migration.version) {
        if applied_checksum != &checksum {
          return Err(AppError::D1(format!("migration {} checksum does not match the applied D1 migration", migration.version)));
        }
        continue;
      }
      self
        .batch(vec![(migration.sql.as_ref(), Vec::new()), ("INSERT INTO _baziflow_migrations (version, checksum) VALUES (?1, ?2)", vec![Value::from(migration.version), Value::String(checksum)])])
        .await?;
    }

    Ok(())
  }
}

fn json_i64(row: &Map<String, Value>, column: &str) -> Option<i64> {
  row.get(column).and_then(|value| value.as_i64().or_else(|| value.as_str().and_then(|value| value.parse().ok())))
}

#[cfg(test)]
mod tests {
  use super::*;

  fn success_response(results: Value) -> String {
    serde_json::json!({"result":[{"results":results,"success":true}],"success":true,"errors":[]}).to_string()
  }

  #[tokio::test]
  async fn sends_parameterized_query_and_reads_rows() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
      .mock("POST", "/accounts/account/d1/database/database/query")
      .match_header("authorization", "Bearer secret")
      .match_body(mockito::Matcher::Json(serde_json::json!({"sql":"SELECT value FROM test WHERE id = ?1","params":["7"]})))
      .with_status(200)
      .with_header("content-type", "application/json")
      .with_body(r#"{"result":[{"results":[{"value":"found"}],"success":true}],"success":true,"errors":[]}"#)
      .create_async()
      .await;
    let config = D1Config {
      account_id: "account".into(),
      database_id: "database".into(),
      api_token: "secret".into(),
      r2_account_id: None,
      r2_access_key_id: None,
      r2_secret_access_key: None,
      r2_bucket_name: None,
    };
    let database = D1Database::new(Client::new(), &config, &server.url());

    let rows = database.fetch_all("SELECT value FROM test WHERE id = ?1", vec![Value::String("7".into())]).await.expect("D1 query should succeed");

    assert_eq!(rows.first().and_then(|row| row.get("value")).and_then(Value::as_str), Some("found"));
    mock.assert_async().await;
  }

  #[tokio::test]
  async fn reports_cloudflare_query_errors() {
    let mut server = mockito::Server::new_async().await;
    let _mock = server
      .mock("POST", "/accounts/account/d1/database/database/query")
      .with_status(400)
      .with_header("content-type", "application/json")
      .with_body(r#"{"result":[],"success":false,"errors":[{"code":7500,"message":"bad query"}]}"#)
      .create_async()
      .await;
    let config = D1Config {
      account_id: "account".into(),
      database_id: "database".into(),
      api_token: "secret".into(),
      r2_account_id: None,
      r2_access_key_id: None,
      r2_secret_access_key: None,
      r2_bucket_name: None,
    };
    let database = D1Database::new(Client::new(), &config, &server.url());

    let error = database.execute("BROKEN", Vec::new()).await.expect_err("D1 query should fail");

    assert!(error.to_string().contains("7500: bad query"));
  }

  #[tokio::test]
  async fn recognizes_migrations_from_the_shared_sqlx_source() {
    let mut server = mockito::Server::new_async().await;
    let create = server
      .mock("POST", "/accounts/account/d1/database/database/query")
      .match_body(mockito::Matcher::Json(serde_json::json!({
        "sql":"CREATE TABLE IF NOT EXISTS _baziflow_migrations (version INTEGER PRIMARY KEY, checksum TEXT NOT NULL, applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S', 'now')))"
      })))
      .with_status(200)
      .with_header("content-type", "application/json")
      .with_body(success_response(serde_json::json!([])))
      .create_async()
      .await;
    let applied = crate::repos::MIGRATOR
      .iter()
      .filter(|migration| !migration.migration_type.is_down_migration())
      .map(|migration| serde_json::json!({"version":migration.version,"checksum":hex::encode(migration.checksum.as_ref())}))
      .collect::<Vec<_>>();
    let select = server
      .mock("POST", "/accounts/account/d1/database/database/query")
      .match_body(mockito::Matcher::Json(serde_json::json!({"sql":"SELECT version, checksum FROM _baziflow_migrations"})))
      .with_status(200)
      .with_header("content-type", "application/json")
      .with_body(success_response(serde_json::json!(applied)))
      .create_async()
      .await;
    let config = D1Config {
      account_id: "account".into(),
      database_id: "database".into(),
      api_token: "secret".into(),
      r2_account_id: None,
      r2_access_key_id: None,
      r2_secret_access_key: None,
      r2_bucket_name: None,
    };
    let database = D1Database::new(Client::new(), &config, &server.url());

    database.apply_migrations().await.expect("Already-applied migrations should be accepted");

    create.assert_async().await;
    select.assert_async().await;
  }
}
