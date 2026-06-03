use teloxide::{prelude::*, types::MessageId};
use tracing::{error, info};

use crate::repos;
use crate::services::paipan;
use crate::utils;

pub async fn perform_bazi_analysis(bot: Bot, chat_id: ChatId, user_id: u64, username: String, message_id: Option<MessageId>) -> ResponseResult<()> {
    let state = crate::models::get_state();
    let user_profile = repos::get_user_profile(&state.db_pool, user_id).await;
    let llm_model = user_profile.llm_model;
    let (birth_date, birth_hour, birth_minute, gender, location, location_status) = if let Some(mut ctx) = state.user_contexts.get_mut(&user_id) {
        let date = ctx.profile_state.birthdate.clone().unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d").to_string());
        let hour = ctx.profile_state.hour.unwrap_or(0);
        let minute = ctx.profile_state.minute.unwrap_or(0);
        let gender = ctx.profile_state.gender.unwrap_or(1);
        let location = ctx.profile_state.location.clone();
        let location_status = if let Some(city) = &location {
            format!("✅ City selected: {}", city)
        } else {
            "✅ Using Standard Time (120°E)".to_string()
        };

        ctx.profile_state = crate::models::state::ProfileState::default();

        (date, hour, minute, gender, location, location_status)
    } else {
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
    };

    let status_text = format!(
        "{}\n✅ Time received.\n\n📅 Date: {}\n🕒 Time: {:02}:{:02}\n⌛ Calculating your Bazi chart...",
        location_status, birth_date, birth_hour, birth_minute
    );

    // Clear old chat context since we have a brand new Bazi profile
    {
        let mut ctx = state.user_contexts.entry(user_id).or_default();
        ctx.messages.clear();
        ctx.last_active = chrono::Utc::now();
    }
    if let Some(msg_id) = message_id {
        let _ = bot.edit_message_text(chat_id, msg_id, status_text).await;
    } else {
        let _ = bot.send_message(chat_id, status_text).await;
    }

    let _ = bot.send_chat_action(chat_id, teloxide::types::ChatAction::Typing).await;

    let params = crate::services::bazi_service::BaziDataParams {
        user_id,
        username: &username,
        birth_date: &birth_date,
        birth_hour,
        birth_minute,
        gender,
        location,
    };

    let structured_data = match crate::services::bazi_service::prepare_bazi_data(&state, params).await {
        Ok(data) => data,
        Err(e) => {
            error!("Failed to fetch bazi chart from API: {}", e);
            let _ = bot.send_message(chat_id, "❌ Error fetching Bazi chart from API. Please try again later.".to_string()).await;
            return Ok(());
        }
    };

    crate::services::bazi_service::build_and_save_bazi_html(&state, user_id, &username, &structured_data).await;

    let chart_url = match crate::services::bazi_service::get_bazi_chart_url(&state, user_id) {
        Ok(url) => url,
        Err(e) => {
            error!("Failed to generate bazi chart URL: {}", e);
            format!("{}/bazi_{}.html", state.config.base_url.trim_end_matches('/'), user_id)
        }
    };
    let _ = bot
        .send_message(chat_id, format!("📊 <b>Bazi Chart Diagram</b>\n\nView your chart here:\n{}", chart_url))
        .parse_mode(teloxide::types::ParseMode::Html)
        .await;

    match crate::services::bazi_service::core_bazi_analysis(&state, user_id, &structured_data, llm_model).await {
        Ok(receiver) => {
            let reading = super::helpers::stream_to_telegram(&bot, chat_id, "🔮 Generating bazi analysis...", receiver).await;
            if !reading.is_empty() {
                repos::save_user_bazi_analysis(&state.db_pool, user_id, &reading).await;

                {
                    let mut ctx = state.user_contexts.entry(user_id).or_default();
                    ctx.push_message(format!("Assistant: {}", reading), state.config.max_context_messages);
                }

                // Generate bazi summary in background
                let state_clone = state.clone();
                tokio::spawn(async move {
                    if let Ok(summary) = crate::services::bazi_service::generate_bazi_summary(&state_clone, user_id, &reading, llm_model).await {
                        repos::save_user_bazi_summary(&state_clone.db_pool, user_id, &summary).await;
                    } else {
                        tracing::error!("Failed to generate bazi summary for user {}", user_id);
                    }
                });
            }
        }
        Err(e) => {
            error!("Failed to generate bazi analysis: {}", e);
            let _ = bot.send_message(chat_id, "❌ Error generating Bazi analysis from AI. Please try again later.".to_string()).await;
        }
    }

    Ok(())
}

/// Query and display the user's Bazi profile and destiny reading.
pub async fn display_user_profile(bot: Bot, chat_id: ChatId, user_id: u64) -> ResponseResult<()> {
    let state = crate::models::get_state();
    let user_profile = repos::get_user_profile(&state.db_pool, user_id).await;

    let Some(user_bazi_four_pillars_raw) = user_profile.bazi_four_pillars.as_deref() else {
        bot.send_message(chat_id, "⚠️ You haven't set your birthdate yet! Please use /new to set your birthdate first to get accurate analysis.")
            .await?;
        return Ok(());
    };

    match serde_json::from_str::<paipan::StructuredBazi>(user_bazi_four_pillars_raw) {
        Ok(structured) => {
            let gender_str = if structured.info.gender == "男,乾造" { "男 (Male)" } else { "女 (Female)" };
            let solar_str = &structured.info.solar_date;
            let lunar_str = &structured.info.lunisolar_date;

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

            let chart_url = match crate::services::bazi_service::get_bazi_chart_url(&state, user_id) {
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
                • <b>Solar Birthday (阳历生日):</b> {solar_str}\n\
                • <b>Lunar Birthday (农历生日):</b> {lunar_str}\n\n\
                <b>Bazi Four Pillars (四柱排盘):</b>\n\
                {pillars_text}\
                <a href=\"{chart_url}\">View Bazi Chart Diagram</a>"
            );

            bot.send_message(chat_id, profile_msg).parse_mode(teloxide::types::ParseMode::Html).await?;

            let llm_model_str = user_profile.llm_model.map(|m| m.as_str()).unwrap_or("Not set (未设置)");
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
        Err(e) => {
            bot.send_message(chat_id, format!("❌ Error reading your Bazi profile data: {}", e)).await?;
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

pub async fn process_date_selection(bot: &Bot, q: &CallbackQuery, formatted_date: &str, request_type: &str, response_prefix: &str) -> ResponseResult<()> {
    let state = crate::models::get_state();
    let user = &q.from;
    let user_id = user.id.0;
    info!("User {} selected date: {}", user_id, formatted_date);

    // Clear history context for a fresh reading
    {
        let mut ctx = state.user_contexts.entry(user_id).or_default();
        ctx.messages.clear();
        ctx.last_active = chrono::Utc::now();
    }

    if let Some(msg) = &q.message {
        let chat_id = msg.chat().id;
        let msg_id = msg.id();
        let _ = bot.edit_message_text(chat_id, msg_id, format!("Processing date: {}", formatted_date)).await;

        let user_profile = repos::get_user_profile(&state.db_pool, user_id).await;
        let bazi_four_pillars = user_profile
            .bazi_four_pillars
            .as_deref()
            .and_then(|raw| serde_json::from_str::<crate::services::paipan::models::StructuredBazi>(raw).ok())
            .map(|b| b.to_string());
        let Some(bazi_four_pillars) = bazi_four_pillars.as_deref() else {
            let _ = bot
                .send_message(chat_id, "⚠️ You haven't set your birthdate yet! Please use /new to set your birthdate first to get accurate readings.")
                .await;
            return Ok(());
        };

        let almanac_data = match crate::services::almanac::fetch_and_format_almanac(&state.http_client, formatted_date).await {
            Ok(data) => data,
            Err(e) => {
                error!("Failed to fetch almanac data: {}", e);
                let _ = bot.send_message(chat_id, format!("Error fetching almanac data: {}", e)).await;
                return Ok(());
            }
        };

        let _ = bot.send_message(chat_id, format!("📅 【{} 黄历信息】\n\n{}", formatted_date, almanac_data)).await;

        match crate::services::almanac::analysis_date_fortune(crate::services::almanac::DateFortuneRequest {
            target_date: formatted_date,
            almanac_data: &almanac_data,
            bazi_four_pillars,
            bazi_summary: user_profile.bazi_summary.as_deref().unwrap_or_else(|| user_profile.bazi_analysis.as_deref().unwrap_or_default()),
            stream: true,
            llm_model: user_profile.llm_model,
            user_id: Some(user_id as i64),
            request_type: Some(request_type.to_string()),
        })
        .await
        {
            Ok(crate::models::LlmResponse::Stream(receiver)) => {
                let result_text = super::helpers::stream_to_telegram(bot, chat_id, response_prefix, receiver).await;

                // Save assistant response to context for future follow-ups
                {
                    let mut ctx = state.user_contexts.entry(user_id).or_default();
                    ctx.push_message(format!("Assistant: {}", result_text), state.config.max_context_messages);
                }
            }
            Ok(_) => {
                error!("Expected stream response from LLM, got non-stream variant");
                let _ = bot.send_message(chat_id, "Internal error: unexpected response type").await;
            }
            Err(e) => {
                error!("Error: {}", e);
                let _ = bot.send_message(chat_id, format!("Error generating reading: {}", e)).await;
            }
        }
    }
    Ok(())
}

pub async fn process_pick_selection(bot: Bot, chat_id: ChatId, user_id: u64, target: Option<MessageTarget>) -> ResponseResult<()> {
    let state = crate::models::get_state();

    let ctx = state.user_contexts.get(&user_id);
    let (start_date, end_date, activity) = match ctx {
        Some(c) => (
            c.pick_state.start_date.clone().unwrap_or_default(),
            c.pick_state.end_date.clone().unwrap_or_default(),
            c.pick_state.activity.clone().unwrap_or_default(),
        ),
        None => return Ok(()),
    };

    if start_date.is_empty() || end_date.is_empty() || activity.is_empty() {
        let _ = bot.send_message(chat_id, "⚠️ Pick selection context is missing. Please restart the process with /pick.").await;
        return Ok(());
    }

    // Clear history context for a fresh reading
    {
        let mut ctx = state.user_contexts.entry(user_id).or_default();
        ctx.messages.clear();
        ctx.last_active = chrono::Utc::now();
    }

    match &target {
        Some(MessageTarget::Edit(m_id)) => {
            let _ = bot
                .edit_message_text(
                    chat_id,
                    *m_id,
                    format!("⏳ Analyzing dates from {} to {} for activity: '{}'...\nThis may take a minute.", start_date, end_date, activity),
                )
                .await;
        }
        Some(MessageTarget::Reply(m_id)) => {
            let _ = bot
                .send_message(
                    chat_id,
                    format!("⏳ Analyzing dates from {} to {} for activity: '{}'...\nThis may take a minute.", start_date, end_date, activity),
                )
                .reply_parameters(teloxide::types::ReplyParameters::new(*m_id))
                .await;
        }
        None => {}
    }

    let user_profile = repos::get_user_profile(&state.db_pool, user_id).await;
    let bazi_four_pillars = user_profile
        .bazi_four_pillars
        .as_deref()
        .and_then(|raw| serde_json::from_str::<crate::services::paipan::models::StructuredBazi>(raw).ok())
        .map(|b| b.to_string());
    let Some(bazi_four_pillars) = bazi_four_pillars.as_deref() else {
        let _ = bot
            .send_message(chat_id, "⚠️ You haven't set your birthdate yet! Please use /new to set your birthdate first to get accurate readings.")
            .await;
        return Ok(());
    };

    let placeholder_text = format!("🎯 择日分析 (Date Selection): '{}'", activity);

    match crate::services::almanac::analysis_pick_selection(crate::services::almanac::PickSelectionRequest {
        start_date: &start_date,
        end_date: &end_date,
        activity: &activity,
        bazi_four_pillars,
        bazi_summary: user_profile.bazi_summary.as_deref().unwrap_or_else(|| user_profile.bazi_analysis.as_deref().unwrap_or_default()),
        stream: true,
        llm_model: user_profile.llm_model,
        user_id: Some(user_id as i64),
        request_type: Some("pick_selection".to_string()),
    })
    .await
    {
        Ok(crate::models::LlmResponse::Stream(receiver)) => {
            let result_text = super::helpers::stream_to_telegram(&bot, chat_id, &placeholder_text, receiver).await;

            // Save assistant response to context for future follow-ups
            {
                let mut ctx = state.user_contexts.entry(user_id).or_default();
                ctx.push_message(format!("Assistant: {}", result_text), state.config.max_context_messages);
            }
        }
        Ok(_) => {
            tracing::error!("Expected stream response from LLM, got non-stream variant");
            let _ = bot.send_message(chat_id, "Internal error: unexpected response type").await;
        }
        Err(e) => {
            tracing::error!("Error: {}", e);
            let _ = bot.send_message(chat_id, format!("Error generating reading: {}", e)).await;
        }
    }

    Ok(())
}
