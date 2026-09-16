//! Bounds third-party chart/almanac responses before deserialization.
use crate::models::AppResult;

pub async fn body(response: reqwest::Response) -> AppResult<Vec<u8>> {
  const LIMIT: usize = 1_048_576;
  let mut response = response.error_for_status()?;
  if response.content_length().is_some_and(|size| size > LIMIT as u64) {
    return Err(anyhow::anyhow!("Upstream response exceeds byte limit").into());
  }
  let mut bytes = Vec::new();
  while let Some(chunk) = response.chunk().await? {
    if bytes.len() + chunk.len() > LIMIT {
      return Err(anyhow::anyhow!("Upstream response exceeds byte limit").into());
    }
    bytes.extend_from_slice(&chunk);
  }
  Ok(bytes)
}

pub async fn json<T: serde::de::DeserializeOwned>(response: reqwest::Response) -> AppResult<T> {
  Ok(serde_json::from_slice(&body(response).await?)?)
}
