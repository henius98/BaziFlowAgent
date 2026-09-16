/// Telegram Bot Handlers & UI Module
///
/// This module orchestrates all interactions with the teloxide Telegram bot.
/// It defines the command structure, inline keyboard callback routing, and
/// integrates with the core business logic in the `services/` layer.
pub mod callbacks;
pub mod command_actions;
pub mod commands;
pub mod helpers;
pub mod keyboards;
pub mod messages;

pub use commands::Command;

pub type Bot = teloxide::adaptors::Throttle<teloxide::Bot>;

/// Bound handler lifetimes and cancel transport work during shutdown.
pub async fn run(future: impl std::future::Future<Output = teloxide::prelude::ResponseResult<()>>) -> teloxide::prelude::ResponseResult<()> {
  let state = crate::models::get_state();
  tokio::select! {
    _ = state.runtime.shutdown.cancelled() => Ok(()),
    result = tokio::time::timeout(std::time::Duration::from_secs(state.config.runtime.work_seconds), future) => {
      match result { Ok(result) => result, Err(_) => { tracing::warn!("Telegram handler deadline exceeded"); Ok(()) } }
    }
  }
}
