use crate::bot::Bot;
use chrono::{Duration, Utc};
use std::sync::Arc;
use teloxide::prelude::*;
use tokio_cron_scheduler::Job;
use tracing::{error, info, warn};

use crate::repos;

/// Configuration for the scheduler (backward-compatible handle wrapper)
pub struct SchedulerConfig {
  pub bot: Bot,
}

// Global scheduler singletons eliminated in favor of AppState dependency injection.
// Scheduler instance is held inside AppState for dynamic additions.

/// Start the background scheduler with:
/// 1. Dynamic daily jobs to generate personalized fortune readings for users with a schedule
/// 2. A cleanup job every 5 minutes to expire old user contexts
pub async fn start_scheduler(state: Arc<crate::models::AppState>, user_contexts_expiration_minutes: i64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
  let sched = match &state.scheduler {
    Some(s) => s.clone(),
    None => return Ok(()),
  };

  let scheduled_users = repos::get_all_scheduled_users(&state.db_pool).await?;

  for (user_id, cron) in scheduled_users {
    add_or_update_user_schedule(&state, user_id, &cron).await;
  }

  // Add cleanup job to run every 5 minutes
  let state_cleanup = state.clone();
  let cleanup_job = Job::new_async_tz(state.config.context_cleanup_cron.as_str(), state.config.app_timezone, move |_uuid, _l| {
    let exp_mins = user_contexts_expiration_minutes;
    let state = state_cleanup.clone();
    Box::pin(async move {
      let now = Utc::now();
      state.user_contexts.retain(|_, ctx| ctx.is_processing || now.signed_duration_since(ctx.last_active).num_minutes() <= exp_mins);
      let wizard_timeout = std::time::Duration::from_secs((exp_mins.max(1) as u64) * 60);
      prune_expired_wizards(&state, wizard_timeout);
    })
  })?;
  sched.add(cleanup_job).await?;

  // Add log cleanup job to run daily
  let log_retention = state.config.log_retention_days;
  let log_cleanup_job = Job::new_async_tz(state.config.log_cleanup_cron.as_str(), state.config.app_timezone, move |_uuid, _l| {
    Box::pin(async move {
      info!("Running daily log cleanup task...");
      let _ = tokio::task::spawn_blocking(move || crate::logger::cleanup_old_logs(log_retention)).await;
    })
  })?;
  sched.add(log_cleanup_job).await?;

  sched.start().await?;
  info!("Scheduler started successfully");

  Ok(())
}

pub async fn add_or_update_user_schedule(state: &Arc<crate::models::AppState>, user_id: u64, cron_str: &str) {
  let sched = match &state.scheduler {
    Some(s) => s,
    None => return,
  };
  let bot = match &state.bot {
    Some(b) => b.clone(),
    None => return,
  };

  // Remove existing if any
  if let Some((_, old_uuid)) = state.user_jobs.remove(&user_id) {
    let _ = sched.remove(&old_uuid).await;
  }

  let state_job = state.clone();
  let daily_job = match Job::new_async_tz(cron_str, state.config.app_timezone, move |_uuid, _l| {
    let bot_clone = bot.clone();
    let state = state_job.clone();
    Box::pin(async move {
      let _tracked = state.runtime.tasks.token();
      let state_work = state.clone();
      let work = async move {
        let Some(_guard) = crate::models::ProcessingGuard::acquire(state_work.clone(), user_id) else {
          warn!(user_id, "Scheduled reading skipped: service busy");
          return;
        };
        let tomorrow = (Utc::now().with_timezone(&state_work.config.app_timezone).date_naive() + Duration::days(1)).format("%Y-%m-%d").to_string();

        match crate::services::almanac::get_daily_fortune(&state_work, user_id, &tomorrow, false, crate::models::LlmRequestType::ScheduledDaily, None).await {
          Ok(result) => {
            info!("Scheduled reading generated for user {}", user_id);

            // Send almanac data message first, back-to-back with the LLM result
            let _ = bot_clone.send_message(ChatId(user_id as i64), format!("📅 【{} 明日黄历】\n\n{}", tomorrow, result.almanac_data)).await;

            if let crate::models::LlmResponse::Full(response) = result.response {
              let parts = crate::utils::split_message(&response, 4000);
              for part in parts {
                if let Err(e) = bot_clone.send_message(ChatId(user_id as i64), part).await {
                  error!("Failed to send scheduled message to user {}: {}", user_id, e);
                  break;
                }
              }
              // Save the daily analysis into chat history so it feeds future conversation context
              if let Err(e) = state_work.chat_cache.save(user_id, crate::repos::traits::CacheRole::Assistant, &response) {
                error!("Failed to save scheduled daily analysis to chat history for user {}: {}", user_id, e);
              }
            } else {
              error!("Unexpected stream response for scheduled job (user {})", user_id);
            }
          }
          Err(crate::models::AppError::ProfileNotFound) => {
            warn!("User {} lacks bazi profile, skipping scheduled job.", user_id);
          }
          Err(e) => {
            error!("Scheduled job error for user {}: {}", user_id, e);
          }
        }
      };
      tokio::select! {
        _ = state.runtime.shutdown.cancelled() => {},
        result = tokio::time::timeout(std::time::Duration::from_secs(state.config.runtime.work_seconds), work) => {
          if result.is_err() { warn!(user_id, "Scheduled reading deadline exceeded"); }
        }
      }
    })
  }) {
    Ok(job) => job,
    Err(e) => {
      error!("Failed to create job for user {}: {}", user_id, e);
      return;
    }
  };

  match sched.add(daily_job).await {
    Ok(uuid) => {
      state.user_jobs.insert(user_id, uuid);
      info!("Successfully add scheduled job for user {} with cron {}", user_id, cron_str);
    }
    Err(e) => {
      error!("Failed to add job to scheduler for user {}: {}", user_id, e);
    }
  }
}

pub async fn remove_user_daily_job(state: &crate::models::AppState, user_id: u64) {
  let sched = match &state.scheduler {
    Some(s) => s,
    None => return,
  };

  if let Some((_, old_uuid)) = state.user_jobs.remove(&user_id) {
    if let Err(e) = sched.remove(&old_uuid).await {
      error!("Failed to remove job for user {}: {}", user_id, e);
    } else {
      info!("Successfully removed scheduled job for user {}", user_id);
    }
  }
}

/// Prune expired wizard entries (profile and pick) based on their `last_active` timestamp.
pub fn prune_expired_wizards(state: &crate::models::AppState, max_age: std::time::Duration) {
  state.profile_wizards.retain(|_, w| w.last_active.elapsed() <= max_age);
  state.pick_wizards.retain(|_, w| w.last_active.elapsed() <= max_age);
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn test_prune_expired_wizards() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new().connect("sqlite::memory:").await.expect("in-memory db connects");
    let config = Arc::new(crate::config::AppConfig {
      runtime: crate::config::runtime::RuntimeConfig { rate_limit: 100, ..Default::default() },
      upstreams: crate::config::Upstreams { chart_base: "http://localhost".into(), supplement_base: "http://localhost".into(), almanac: "http://localhost/api/almanac".into() },
      telegram_bot_token: "".into(),
      telegram_webhook_enabled: false,
      telegram_webhook_path: "/telegram/webhook".into(),
      llm_client_config: crate::services::llm::LlmClientConfig { api_key: "test".into(), api_base: "http://localhost".into(), timeout_seconds: 30, http_client: None },
      database_url: "sqlite::memory:".into(),
      d1: None,
      chat_cache_database_url: "sqlite::memory:".into(),
      chat_cache_max_messages: 40,
      user_contexts_expiration_minutes: 60,
      context_cleanup_cron: "".into(),
      log_cleanup_cron: "".into(),
      app_timezone: chrono_tz::Tz::UTC,
      log_retention_days: 7,
      max_context_messages: 10,
      base_url: "http://localhost".into(),
      log_level: "info".into(),
      cors_allowed_origin: "*".into(),
      chart_token_secret: [7u8; 32],
    });
    let chat_cache = crate::repos::chat_history_cache::init_chat_history_cache(&config.chat_cache_database_url, config.chat_cache_max_messages).await.expect("init chat cache");
    let state = crate::models::AppState::new(reqwest::Client::new(), pool, chat_cache.pool, chat_cache.writer, config, None, None).expect("create app state");

    let fresh_profile = crate::models::state::ProfileState { last_active: std::time::Instant::now(), ..Default::default() };
    state.profile_wizards.insert(1, fresh_profile);

    let expired_profile =
      crate::models::state::ProfileState { last_active: std::time::Instant::now().checked_sub(std::time::Duration::from_secs(3600)).expect("valid expired instant"), ..Default::default() };
    state.profile_wizards.insert(2, expired_profile);

    let fresh_pick = crate::models::state::PickState { last_active: std::time::Instant::now(), ..Default::default() };
    state.pick_wizards.insert(1, fresh_pick);

    let expired_pick =
      crate::models::state::PickState { last_active: std::time::Instant::now().checked_sub(std::time::Duration::from_secs(3600)).expect("valid expired instant"), ..Default::default() };
    state.pick_wizards.insert(2, expired_pick);

    prune_expired_wizards(&state, std::time::Duration::from_secs(900));

    assert_eq!(state.profile_wizards.len(), 1);
    assert!(state.profile_wizards.contains_key(&1));
    assert!(!state.profile_wizards.contains_key(&2));

    assert_eq!(state.pick_wizards.len(), 1);
    assert!(state.pick_wizards.contains_key(&1));
    assert!(!state.pick_wizards.contains_key(&2));
  }
}
