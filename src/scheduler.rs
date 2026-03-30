use chrono::{Datelike, Duration, Local, NaiveDate, Utc};
use chrono_tz::Asia::Singapore;
use std::sync::Arc;
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{error, info};

use crate::dify;

/// Configuration for the scheduler
pub struct SchedulerConfig {
    pub webhook_url: String,
    pub http_client: reqwest::Client,
}

/// Start the background scheduler with:
/// 1. A daily job at 10:00 PM SGT to send tomorrow's date to Dify
/// 2. A cleanup job every 5 minutes to expire old user contexts
pub async fn start_scheduler(
    config: Arc<SchedulerConfig>,
    user_contexts: Arc<dashmap::DashMap<i64, Vec<String>>>,
    user_last_active: Arc<dashmap::DashMap<i64, chrono::DateTime<Utc>>>,
    expiration_minutes: i64,
) -> Result<JobScheduler, Box<dyn std::error::Error + Send + Sync>> {
    let sched = JobScheduler::new().await?;

    // Add job to run daily at 10:00 pm SGT (14:00 UTC)
    // Cron: sec min hour day month weekday
    let config_clone = config.clone();
    let daily_job = Job::new_async("0 0 14 * * *", move |_uuid, _l| {
        let cfg = config_clone.clone();
        Box::pin(async move {
            info!("Running scheduled Dify job...");
            let tomorrow = (Local::now().date_naive() + Duration::days(1))
                .format("%Y-%m-%d")
                .to_string();

            // We pass empty history_msg since it's a scheduled job
            match dify::send_to_dify(&cfg.http_client, &cfg.webhook_url, &tomorrow, "").await {
                Ok(response) => {
                    info!("Scheduled Job Response: {:?}", response);
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
