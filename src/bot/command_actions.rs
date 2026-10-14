use super::Bot;
use teloxide::{prelude::*, types::MessageId};
use tracing::{error, info};

use crate::repos;
use crate::utils;

pub async fn perform_bazi_analysis(bot: Bot, state: &std::sync::Arc<crate::models::AppState>, chat_id: ChatId, user_id: u64, username: String, message_id: Option<MessageId>) -> ResponseResult<()> {
  let user_profile = repos::get_user_profile(&state.db_pool, user_id).await.unwrap_or_default();
  let llm_model = user_profile.llm_model;
  let profile_state = match state.profile_wizards.remove(&user_id) {
    Some((_, profile_state)) => profile_state,
    None => {
      error!("Missing user context data for user_id: {}", user_id);
      let msg = "⚠️ Session expired or missing data. Please start again with /new.";
      if let Some(msg_id) = message_id {
        if let Err(e) = bot.edit_message_text(chat_id, msg_id, msg).await {
          error!("Failed to edit message: {}", e);
        }
      } else if let Err(e) = bot.send_message(chat_id, msg).await {
        error!("Failed to send message: {}", e);
      }
      return Ok(());
    }
  };

  let (birth_date, gender) = match (profile_state.birthdate, profile_state.gender) {
    (Some(date), Some(gender)) => (date, gender),
    _ => {
      error!("Incomplete profile state for user_id: {}", user_id);
      let msg = "⚠️ Required profile data is incomplete. Please start again with /new.";
      if let Some(msg_id) = message_id {
        let _ = bot.edit_message_text(chat_id, msg_id, msg).await;
      } else {
        let _ = bot.send_message(chat_id, msg).await;
      }
      return Ok(());
    }
  };

  let birth_time = match (profile_state.hour, profile_state.minute) {
    (Some(hour), Some(minute)) => match chrono::NaiveTime::from_hms_opt(hour as u32, minute as u32, 0) {
      Some(time) => Some(time),
      None => {
        error!("Invalid profile time for user_id: {}", user_id);
        let msg = "⚠️ The selected birth time is invalid. Please start again with /new.";
        if let Some(msg_id) = message_id {
          let _ = bot.edit_message_text(chat_id, msg_id, msg).await;
        } else {
          let _ = bot.send_message(chat_id, msg).await;
        }
        return Ok(());
      }
    },
    (None, None) => None,
    _ => {
      error!("Partially selected profile time for user_id: {}", user_id);
      let msg = "⚠️ Birth time is incomplete. Please start again with /new.";
      if let Some(msg_id) = message_id {
        let _ = bot.edit_message_text(chat_id, msg_id, msg).await;
      } else {
        let _ = bot.send_message(chat_id, msg).await;
      }
      return Ok(());
    }
  };
  let location = profile_state.location;
  let location_status = if let Some(city) = &location { format!("✅ City selected: {}", city) } else { "⏩ Birth location skipped; no longitude correction will be applied.".to_string() };
  let time_status = birth_time.map_or_else(|| "⏩ Birth time skipped; the hour pillar will be masked with *.".to_string(), |time| format!("✅ Time received: {}", time.format("%H:%M")));

  let status_text = format!("{}\n{}\n\n📅 Date: {}\n⌛ Calculating your Bazi chart...", location_status, time_status, birth_date);

  if let Some(msg_id) = message_id {
    let _ = bot.edit_message_text(chat_id, msg_id, status_text).await;
  } else {
    let _ = bot.send_message(chat_id, status_text).await;
  }

  let _ = bot.send_chat_action(chat_id, teloxide::types::ChatAction::Typing).await;

  let params = crate::services::bazi_service::DestinyProfileParams { user_id, username: &username, birth_date: &birth_date, birth_time, gender, location, llm_model };

  let profile_res = match crate::services::bazi_service::create_destiny_profile(state, params).await {
    Ok(res) => res,
    Err(e) => {
      error!("Failed to create destiny profile: {}", e);
      let _ = bot.send_message(chat_id, "❌ Error creating Bazi profile from upstream. Please try again later.".to_string()).await;
      return Ok(());
    }
  };

  let _ = bot.send_message(chat_id, format!("📊 <b>Bazi Chart Diagram</b>\n\nView your chart here:\n{}", profile_res.chart_url)).parse_mode(teloxide::types::ParseMode::Html).await;

  super::helpers::stream_to_telegram(&bot, state, chat_id, "🔮 Generating bazi analysis...", profile_res.stream).await;

  Ok(())
}

/// Query and display the user's Bazi profile and destiny reading.
pub async fn display_user_profile(bot: Bot, state: &std::sync::Arc<crate::models::AppState>, chat_id: ChatId, user_id: u64) -> ResponseResult<()> {
  let user_profile = match repos::get_user_profile(&state.db_pool, user_id).await {
    Ok(p) => p,
    Err(e) => {
      error!("Failed to fetch user profile: {}", e);
      bot.send_message(chat_id, "⚠️ Database error fetching profile. Please try again later.").await?;
      return Ok(());
    }
  };

  let structured = match crate::services::paipan::models::StructuredBazi::from_json(user_profile.bazi_four_pillars.as_deref()) {
    Ok(s) => s,
    Err(_) => {
      bot.send_message(chat_id, "⚠️ You haven't set your birthdate yet or the data is invalid! Please use /new to set your birthdate first to get accurate analysis.").await?;
      return Ok(());
    }
  };
  {
    let gender_str = if structured.info.gender == "男,乾造" { "男 (Male)" } else { "女 (Female)" };
    let solar_str = if structured.info.birth_time_known { structured.info.solar_date.clone() } else { format!("{} (12:00 reference; birth time unknown)", structured.info.solar_date) };
    let lunar_str = &structured.info.lunisolar_date;
    let birth_location_str =
      if structured.info.birth_location == "未知" { "Unknown / skipped".to_string() } else { structured.info.birth_location.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;") };

    let mut pillars_text = String::new();
    for p in &structured.pillars {
      let eng_name = match p.name.as_str() {
        "年柱" => "Year",
        "月柱" => "Month",
        "日柱" => "Day",
        "时柱" => "Hour",
        _ => "",
      };
      let main_star = p.base.stem_and_stars.first().map(|(_, s)| s.as_str()).unwrap_or("");
      let stem = p.base.stem_and_stars.first().map(|(s, _)| s.as_str()).unwrap_or("");
      let branch_stars = p.base.hidden_stems_and_stars.iter().map(|(stem, star)| format!("{}/{}", stem, star)).collect::<Vec<_>>().join(", ");

      pillars_text.push_str(&format!(
        "• <b>{name} ({eng_name}):</b>  [{main_star}] |<u>{stem}{branch}</u>| [{branch_stars}] (<tg-spoiler>{nayin}</tg-spoiler>)\n",
        name = p.name,
        branch = p.base.branch,
        nayin = p.base.nayin
      ));
    }

    let chart_url = match crate::services::bazi_service::get_bazi_chart_url(state, user_id).await {
      Ok(url) => url,
      Err(e) => {
        error!("Failed to generate bazi chart URL: {}", e);
        format!("{}/bazi_{}.html", state.config.base_url.trim_end_matches('/'), user_id)
      }
    };

    let profile_msg = format!(
      "👤 <b>Your Bazi Profile (个人八字档案)</b>\n\n\
                <b>Basic Information (基本信息):</b>\n\
                • <b>Gender (性别):</b> {gender_str}\n\
                • <b>Birth Location (出生地):</b> {birth_location_str}\n\
                • <b>Solar Birthday (阳历生日):</b> {solar_str}\n\
                • <b>Lunar Birthday (农历生日):</b> {lunar_str}\n\n\
                <b>Bazi Four Pillars (四柱排盘):</b>\n\
                {pillars_text}\
                <a href=\"{chart_url}\">View Bazi Chart Diagram</a>"
    );

    bot.send_message(chat_id, profile_msg).parse_mode(teloxide::types::ParseMode::Html).await?;

    let llm_model_str = user_profile.llm_model.as_ref().map(|m| m.as_str()).unwrap_or("Not set (未设置)");
    let schedule_str = user_profile.schedule.as_deref().unwrap_or("Not set (未设置)");
    let settings_msg = format!(
      "⚙️ <b>Settings (设置):</b>\n\
                • <b>LLM Model (AI模型):</b> {llm_model_str}\n\
                • <b>Schedule (每日推送):</b> {schedule_str}"
    );
    bot.send_message(chat_id, settings_msg).parse_mode(teloxide::types::ParseMode::Html).await?;

    if let Some(reading) = user_profile.bazi_analysis.filter(|r| !r.is_empty()) {
      let parts = utils::split_message(&reading, 4000);
      for part in parts {
        // TODO: enhance markdown format
        // bot.send_message(chat_id, part).parse_mode(teloxide::types::ParseMode::MarkdownV2).await?;
        bot.send_message(chat_id, part).await?;
      }
    }
  }

  Ok(())
}

// ─────────────────────────────────────────────
// Message Target Enum
// ─────────────────────────────────────────────
pub enum MessageTarget {
  Edit(teloxide::types::MessageId),
  Reply(teloxide::types::MessageId),
}

pub async fn process_date_selection(
  bot: &Bot,
  state: &std::sync::Arc<crate::models::AppState>,
  q: &CallbackQuery,
  formatted_date: &str,
  request_type: crate::models::LlmRequestType,
  response_prefix: &str,
  reasoning_effort: Option<crate::models::ReasoningEffortLevel>,
) -> ResponseResult<()> {
  let user = &q.from;
  let user_id = user.id.0;
  info!("User {} selected date: {}", user_id, formatted_date);

  if let Some(msg) = &q.message {
    let chat_id = msg.chat().id;
    let msg_id = msg.id();
    let _ = bot.edit_message_text(chat_id, msg_id, format!("Processing date: {}", formatted_date)).await;

    match crate::services::almanac::get_daily_fortune(state, user_id, formatted_date, true, request_type, reasoning_effort).await {
      Ok(fortune) => {
        let _ = bot.send_message(chat_id, format!("📅 【{} 黄历信息】\n\n{}", formatted_date, fortune.almanac_data)).await;

        match fortune.response {
          crate::models::LlmResponse::Stream(receiver) => {
            super::helpers::stream_to_telegram(bot, state, chat_id, response_prefix, receiver).await;
          }
          _ => {
            error!("Expected stream response from LLM, got non-stream variant");
            let _ = bot.send_message(chat_id, "Internal error: unexpected response type").await;
          }
        }
      }
      Err(crate::models::AppError::ProfileNotFound) => {
        let _ = bot.send_message(chat_id, "⚠️ You haven't set your birthdate yet! Please use /new to set your birthdate first to get accurate readings.").await;
      }
      Err(e) => {
        error!("Error fetching date fortune: {}", e);
        let _ = bot.send_message(chat_id, format!("Error generating reading: {}", e)).await;
      }
    }
  }
  Ok(())
}

pub async fn process_pick_selection(
  bot: Bot,
  state: &std::sync::Arc<crate::models::AppState>,
  chat_id: ChatId,
  user_id: u64,
  target: Option<MessageTarget>,
  reasoning_effort: Option<crate::models::ReasoningEffortLevel>,
) -> ResponseResult<()> {
  let (start_date, end_date, activity) = match state.pick_wizards.remove(&user_id) {
    Some((_, c)) => (c.start_date.unwrap_or_default(), c.end_date.unwrap_or_default(), c.activity.unwrap_or_default()),
    None => return Ok(()),
  };

  if start_date.is_empty() || end_date.is_empty() || activity.is_empty() {
    let _ = bot.send_message(chat_id, "⚠️ Pick selection context is missing. Please restart the process with /pick.").await;
    return Ok(());
  }

  match &target {
    Some(MessageTarget::Edit(m_id)) => {
      let _ = bot.edit_message_text(chat_id, *m_id, format!("⏳ Analyzing dates from {} to {} for activity: '{}'...\nThis may take a minute.", start_date, end_date, activity)).await;
    }
    Some(MessageTarget::Reply(m_id)) => {
      let _ = bot
        .send_message(chat_id, format!("⏳ Analyzing dates from {} to {} for activity: '{}'...\nThis may take a minute.", start_date, end_date, activity))
        .reply_parameters(teloxide::types::ReplyParameters::new(*m_id))
        .await;
    }
    None => {}
  }

  let placeholder_text = format!("🎯 择日分析 (Date Selection): '{}'", activity);

  let params = crate::services::almanac::DateSelectionParams {
    user_id,
    start_date: &start_date,
    end_date: &end_date,
    activity: &activity,
    stream: true,
    request_type: crate::models::LlmRequestType::PickSelection,
    reasoning_effort,
  };

  match crate::services::almanac::pick_dates(state, params).await {
    Ok(result) => match result.response {
      crate::models::LlmResponse::Stream(receiver) => {
        super::helpers::stream_to_telegram(&bot, state, chat_id, &placeholder_text, receiver).await;
      }
      _ => {
        tracing::error!("Expected stream response from LLM, got non-stream variant");
        let _ = bot.send_message(chat_id, "Internal error: unexpected response type").await;
      }
    },
    Err(crate::models::AppError::ProfileNotFound) => {
      let _ = bot.send_message(chat_id, "⚠️ You haven't set your birthdate yet! Please use /new to set your birthdate first to get accurate readings.").await;
    }
    Err(e) => {
      tracing::error!("Error: {}", e);
      let _ = bot.send_message(chat_id, format!("Error generating reading: {}", e)).await;
    }
  }

  Ok(())
}
