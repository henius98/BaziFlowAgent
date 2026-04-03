use chrono::{Duration, Local, Utc};
use std::sync::Arc;
use teloxide::prelude::*;
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{error, info};

use crate::state::AppState;
use crate::llm_bazi;

/// Configuration for the scheduler
pub struct SchedulerConfig {
    pub http_client: reqwest::Client,
    pub bot: Bot,
    pub app_state: Arc<AppState>,
    pub admin_chat_id: i64,
}

/// Start the background scheduler with:
/// 1. A daily job at 10:00 PM SGT to send tomorrow's date to LLM
/// 2. A cleanup job every 5 minutes to expire old user contexts
pub async fn start_scheduler(
    config: Arc<SchedulerConfig>,
    user_contexts: Arc<dashmap::DashMap<i64, Vec<String>>>,
    user_last_active: Arc<dashmap::DashMap<i64, chrono::DateTime<Utc>>>,
    expiration_minutes: i64,
) -> Result<JobScheduler, Box<dyn std::error::Error + Send + Sync>> {
    let sched = JobScheduler::new().await?;

    let config_clone = config.clone();
    let daily_job = Job::new_async("0 0 14 * * *", move |_uuid, _l| {
        let cfg = config_clone.clone();
        Box::pin(async move {
            info!("Running scheduled Bazi job...");
            let tomorrow = (Local::now().date_naive() + Duration::days(1))
                .format("%Y-%m-%d")
                .to_string();

            // We pass empty history_msg since it's a scheduled job
            match llm_bazi::generate_bazi_reading(
                &cfg.http_client,
                &tomorrow,
                "",
                &cfg.app_state.user_bazi,
                &cfg.app_state.openai_api_key,
                &cfg.app_state.openai_api_base,
                &cfg.app_state.llm_model_name,
            )
            .await
            {
                Ok(response) => {
                    info!("Scheduled Job LLM generated");
                    if let Err(e) = cfg
                        .bot
                        .send_message(ChatId(cfg.admin_chat_id), response)
                        .await
                    {
                        error!(
                            "Failed to send scheduled message to admin {}: {}",
                            cfg.admin_chat_id, e
                        );
                    }
                }
                Err(e) => {
                    error!("Scheduled Job Error: {}", e);
                }
            }
        })
    })?;
    sched.add(daily_job).await?;

    // Add cleanup job to run every 5 minutes
    let cleanup_job = Job::new_async("0 */5 * * * *", move |_uuid, _l| {
        let contexts = user_contexts.clone();
        let last_active = user_last_active.clone();
        let exp_mins = expiration_minutes;
        Box::pin(async move {
            let now = Utc::now();
            let mut expired_users: Vec<i64> = Vec::new();

            for entry in last_active.iter() {
                let user_id = *entry.key();
                let last = *entry.value();
                if now.signed_duration_since(last).num_minutes() > exp_mins {
                    expired_users.push(user_id);
                }
            }

            for user_id in expired_users {
                contexts.remove(&user_id);
                last_active.remove(&user_id);
                info!("Cleaned up expired context for user: {}", user_id);
            }
        })
    })?;
    sched.add(cleanup_job).await?;

    sched.start().await?;
    info!("Scheduler started successfully");

    Ok(sched)
}
