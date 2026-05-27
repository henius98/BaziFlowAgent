use chrono::{Duration, Local, Utc};
use std::sync::Arc;
use teloxide::prelude::*;
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{error, info};

use crate::models::AppState;
use crate::repos;
use crate::services::llm_bazi;

/// Configuration for the scheduler
pub struct SchedulerConfig {
    pub http_client: reqwest::Client,
    pub bot: Bot,
    pub app_state: Arc<AppState>,
}

/// Start the background scheduler with:
/// 1. A daily job to generate personalized fortune readings for all profiled users
/// 2. A cleanup job every 5 minutes to expire old user contexts
pub async fn start_scheduler(config: Arc<SchedulerConfig>, user_contexts_expiration_minutes: i64) -> Result<JobScheduler, Box<dyn std::error::Error + Send + Sync>> {
    let sched = JobScheduler::new().await?;

    let daily_cfg = config.clone();
    let daily_job = Job::new_async(config.app_state.config.bazi_job_cron.as_str(), move |_uuid, _l| {
        let cfg = daily_cfg.clone();
        Box::pin(async move {
            info!("Running scheduled Bazi job...");
            let tomorrow = (Local::now().date_naive() + Duration::days(1)).format("%Y-%m-%d").to_string();

            // Fetch all users with bazi profiles from database
            let users = repos::get_all_users_with_bazi(&cfg.app_state.db_pool).await;
            if users.is_empty() {
                info!("No users with bazi profiles found, skipping scheduled job.");
                return;
            }

            info!("Generating fortune readings for {} user(s)...", users.len());

            for (user_id, bazi_four_pillars, destiny_reading) in &users {
                // let formatted_bazi = crate::utils::get_formatted_bazi_four_pillars(bazi_four_pillars);
                let destiny = destiny_reading.as_deref().unwrap_or("");

                match llm_bazi::generate_bazi_reading(llm_bazi::BaziReadingRequest {
                    http_client: &cfg.http_client,
                    date_value: &tomorrow,
                    history_msg: "",
                    user_bazi_four_pillars: &bazi_four_pillars,
                    destiny_reading: destiny,
                    api_key: &cfg.app_state.config.openai_api_key,
                    api_base: &cfg.app_state.config.openai_api_base,
                    model_name: &cfg.app_state.config.llm_model_name,
                })
                .await
                {
                    Ok(response) => {
                        info!("Scheduled reading generated for user {}", user_id);
                        let parts = crate::utils::split_message(&response, 4000);
                        for part in parts {
                            if let Err(e) = cfg.bot.send_message(ChatId(*user_id as i64), part).await {
                                error!("Failed to send scheduled message to user {}: {}", user_id, e);
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        error!("Scheduled job error for user {}: {}", user_id, e);
                    }
                }
            }
        })
    })?;
    sched.add(daily_job).await?;

    // Add cleanup job to run every 5 minutes
    let cleanup_cfg = config.clone();
    let cleanup_job = Job::new_async(config.app_state.config.context_cleanup_cron.as_str(), move |_uuid, _l| {
        let state = cleanup_cfg.app_state.clone();
        let exp_mins = user_contexts_expiration_minutes;
        Box::pin(async move {
            let now = Utc::now();
            let mut expired_users: Vec<u64> = Vec::new();

            for entry in state.user_contexts.iter() {
                let user_id = *entry.key();
                let last = entry.value().last_active;
                if now.signed_duration_since(last).num_minutes() > exp_mins {
                    expired_users.push(user_id);
                }
            }

            for user_id in expired_users {
                state.user_contexts.remove(&user_id);
                info!("Cleaned up expired context for user: {}", user_id);
            }
        })
    })?;
    sched.add(cleanup_job).await?;

    // Add log cleanup job to run daily
    let log_retention = config.app_state.config.log_retention_days;
    let log_cleanup_job = Job::new(config.app_state.config.log_cleanup_cron.as_str(), move |_uuid, _l| {
        info!("Running daily log cleanup task...");
        crate::logger::cleanup_old_logs(log_retention);
    })?;
    sched.add(log_cleanup_job).await?;

    sched.start().await?;
    info!("Scheduler started successfully");

    Ok(sched)
}
