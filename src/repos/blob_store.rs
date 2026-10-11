//! BlobStore implementation for local filesystem and Cloudflare R2 / S3.

use crate::config::AppConfig;
use crate::models::error::{AppError, AppResult};
use crate::repos::traits::BlobStore;
use futures::future::BoxFuture;
use std::path::PathBuf;
use std::sync::Arc;

/// Local filesystem blob storage serving capability tokens.
#[derive(Debug, Clone)]
pub struct LocalFsBlobStore {
  pub base_dir: PathBuf,
  pub base_url: String,
  pub chart_token_secret: [u8; 32],
}

impl LocalFsBlobStore {
  pub fn new(base_dir: impl Into<PathBuf>, base_url: impl Into<String>, secret: [u8; 32]) -> Self {
    Self { base_dir: base_dir.into(), base_url: base_url.into(), chart_token_secret: secret }
  }

  fn extract_user_id(&self, key: &str) -> Option<u64> {
    // Expected key format: "bazi_{user_id}.html"
    let stem = key.strip_prefix("bazi_")?.strip_suffix(".html")?;
    stem.parse().ok()
  }
}

impl BlobStore for LocalFsBlobStore {
  fn put<'a>(&'a self, key: &'a str, data: &'a [u8], _content_type: Option<&'a str>) -> BoxFuture<'a, AppResult<()>> {
    Box::pin(async move {
      let path = self.base_dir.join(key);
      if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
      }
      tokio::fs::write(&path, data).await?;
      Ok(())
    })
  }

  fn get<'a>(&'a self, key: &'a str) -> BoxFuture<'a, AppResult<Vec<u8>>> {
    Box::pin(async move {
      let path = self.base_dir.join(key);
      let bytes = tokio::fs::read(&path).await?;
      Ok(bytes)
    })
  }

  fn exists<'a>(&'a self, key: &'a str) -> BoxFuture<'a, AppResult<bool>> {
    Box::pin(async move {
      let path = self.base_dir.join(key);
      Ok(tokio::fs::try_exists(&path).await.unwrap_or(false))
    })
  }

  fn presign_get<'a>(&'a self, key: &'a str, _expiry_secs: u64) -> BoxFuture<'a, AppResult<String>> {
    Box::pin(async move {
      let user_id = self.extract_user_id(key).ok_or_else(|| AppError::Message(format!("Cannot generate capability token for non-chart key: {}", key)))?;
      let token = crate::services::chart_token::generate_chart_token(user_id, &self.chart_token_secret)?;
      Ok(format!("{}/charts/{}", self.base_url.trim_end_matches('/'), token))
    })
  }

  fn delete<'a>(&'a self, key: &'a str) -> BoxFuture<'a, AppResult<()>> {
    Box::pin(async move {
      let path = self.base_dir.join(key);
      if tokio::fs::try_exists(&path).await.unwrap_or(false) {
        tokio::fs::remove_file(&path).await?;
      }
      Ok(())
    })
  }
}

/// Cloudflare R2 / S3 blob storage with SigV4 presigned URLs.
#[derive(Debug)]
pub struct R2BlobStore {
  bucket: Box<s3::Bucket>,
}

impl R2BlobStore {
  pub fn new(bucket: Box<s3::Bucket>) -> Self {
    Self { bucket }
  }
}

impl BlobStore for R2BlobStore {
  fn put<'a>(&'a self, key: &'a str, data: &'a [u8], content_type: Option<&'a str>) -> BoxFuture<'a, AppResult<()>> {
    Box::pin(async move {
      if let Some(mime) = content_type {
        self.bucket.put_object_with_content_type(key, data, mime).await?;
      } else {
        self.bucket.put_object(key, data).await?;
      }
      Ok(())
    })
  }

  fn get<'a>(&'a self, key: &'a str) -> BoxFuture<'a, AppResult<Vec<u8>>> {
    Box::pin(async move {
      let response = self.bucket.get_object(key).await?;
      Ok(response.into_bytes().to_vec())
    })
  }

  fn exists<'a>(&'a self, key: &'a str) -> BoxFuture<'a, AppResult<bool>> {
    Box::pin(async move {
      let head = self.bucket.head_object(key).await;
      Ok(matches!(head, Ok((_, 200..=299))))
    })
  }

  fn presign_get<'a>(&'a self, key: &'a str, expiry_secs: u64) -> BoxFuture<'a, AppResult<String>> {
    Box::pin(async move {
      let url = self.bucket.presign_get(key, expiry_secs as u32, None).await?;
      Ok(url)
    })
  }

  fn delete<'a>(&'a self, key: &'a str) -> BoxFuture<'a, AppResult<()>> {
    Box::pin(async move {
      self.bucket.delete_object(key).await?;
      Ok(())
    })
  }
}

/// Initializes the configured BlobStore implementation.
pub fn init_blob_store(config: &AppConfig) -> AppResult<Arc<dyn BlobStore>> {
  if let Some(d1_config) = &config.d1
    && let (Some(account_id), Some(access_key), Some(secret_key), Some(bucket_name)) =
      (&d1_config.r2_account_id, &d1_config.r2_access_key_id, &d1_config.r2_secret_access_key, &d1_config.r2_bucket_name)
  {
    let creds = s3::creds::Credentials::new(Some(access_key), Some(secret_key), None, None, None)?;
    let region = s3::region::Region::Custom { region: "auto".to_owned(), endpoint: format!("https://{}.r2.cloudflarestorage.com", account_id) };
    let bucket = s3::Bucket::new(bucket_name, region, creds)?.with_path_style();
    Ok(Arc::new(R2BlobStore::new(bucket)))
  } else {
    Ok(Arc::new(LocalFsBlobStore::new("public", config.base_url.clone(), config.chart_token_secret)))
  }
}
