use chrono::{Duration, Local, Utc};
use std::sync::Arc;
use teloxide::prelude::*;
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{error, info, warn};

use crate::repos;

/// Configuration for the scheduler
pub struct SchedulerConfig {
    pub bot: Bot,
}

use std::sync::OnceLock;

pub static GLOBAL_SCHEDULER: OnceLock<JobScheduler> = OnceLock::new();

/// Start the background scheduler with:
/// 1. Dynamic daily jobs to generate personalized fortune readings for users with a schedule
/// 2. A cleanup job every 5 minutes to expire old user contexts
pub async fn start_scheduler(config: Arc<SchedulerConfig>, user_contexts_expiration_minutes: i64) -> Result<JobScheduler, Box<dyn std::error::Error + Send + Sync>> {
    let sched = JobScheduler::new().await?;

    // Initialize global scheduler for dynamic additions
    let _ = GLOBAL_SCHEDULER.set(sched.clone());

    let state = crate::models::get_state();
    let scheduled_users = repos::get_all_scheduled_users(&state.db_pool).await;

    for (user_id, cron) in scheduled_users {
        add_or_update_user_schedule(config.bot.clone(), user_id, &cron).await;
    }

    // Add cleanup job to run every 5 minutes
    let cleanup_job = Job::new_async_tz(crate::models::get_state().config.context_cleanup_cron.as_str(), crate::models::get_state().config.app_timezone, move |_uuid, _l| {
        let exp_mins = user_contexts_expiration_minutes;
        Box::pin(async move {
            let state = crate::models::get_state();
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
    let log_retention = crate::models::get_state().config.log_retention_days;
    let log_cleanup_job = Job::new_tz(crate::models::get_state().config.log_cleanup_cron.as_str(), crate::models::get_state().config.app_timezone, move |_uuid, _l| {
        info!("Running daily log cleanup task...");
        crate::logger::cleanup_old_logs(log_retention);
    })?;
    sched.add(log_cleanup_job).await?;

    sched.start().await?;
    info!("Scheduler started successfully");

    Ok(sched)
}

pub async fn add_or_update_user_schedule(bot: Bot, user_id: u64, cron_str: &str) {
    let state = crate::models::get_state();
    let sched = match GLOBAL_SCHEDULER.get() {
        Some(s) => s,
        None => return,
    };

    // Remove existing if any
    if let Some((_, old_uuid)) = state.user_jobs.remove(&user_id) {
        let _ = sched.remove(&old_uuid).await;
    }

    let daily_job = match Job::new_async_tz(cron_str, crate::models::get_state().config.app_timezone, move |_uuid, _l| {
        let bot_clone = bot.clone();
        Box::pin(async move {
            let state = crate::models::get_state();
            let tomorrow = (Local::now().date_naive() + Duration::days(1)).format("%Y-%m-%d").to_string();

            let user_profile = repos::get_user_profile(&state.db_pool, user_id).await;
            let bazi_four_pillars = match user_profile.bazi_four_pillars {
                Some(p) => p,
                None => {
                    warn!("User {} lacks bazi profile, skipping scheduled job.", user_id);
                    return;
                }
            };

            let almanac_data = match crate::services::almanac::fetch_and_format_almanac(&state.http_client, &tomorrow).await {
                Ok(data) => data,
                Err(e) => {
                    error!("Failed to fetch almanac data for user {} scheduled job: {}", user_id, e);
                    return;
                }
            };

            let bazi_summary = user_profile.bazi_summary.unwrap_or_else(|| user_profile.bazi_analysis.unwrap_or_default());

            match crate::services::almanac::analysis_date_fortune(crate::services::almanac::DateFortuneRequest {
                target_date: &tomorrow,
                almanac_data: &almanac_data,
                bazi_four_pillars: &bazi_four_pillars,
                bazi_summary: &bazi_summary,
                stream: false,
                llm_model: user_profile.llm_model,
                user_id: Some(user_id as i64),
                request_type: Some("scheduled_daily".to_string()),
            })
            .await
            {
                Ok(crate::models::LlmResponse::Full(response)) => {
                    info!("Scheduled reading generated for user {}", user_id);

                    // Send almanac data message first, back-to-back with the LLM result
                    let _ = bot_clone.send_message(ChatId(user_id as i64), format!("📅 【{} 明日黄历】\n\n{}", tomorrow, almanac_data)).await;

                    let parts = crate::utils::split_message(&response, 4000);
                    for part in parts {
                        if let Err(e) = bot_clone.send_message(ChatId(user_id as i64), part).await {
                            error!("Failed to send scheduled message to user {}: {}", user_id, e);
                            break;
                        }
                    }
                }
                Ok(_) => {
                    error!("Unexpected stream response for scheduled job (user {})", user_id);
                }
                Err(e) => {
                    error!("Scheduled job error for user {}: {}", user_id, e);
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

pub async fn remove_user_daily_job(user_id: u64) {
    let state = crate::models::get_state();
    let sched = match GLOBAL_SCHEDULER.get() {
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
