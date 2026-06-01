use teloxide::{prelude::*, types::MessageId};
use tracing::error;

use crate::repos;
use crate::services::paipan;
use crate::utils;

pub async fn perform_bazi_analysis(bot: Bot, chat_id: ChatId, user_id: u64, username: String, message_id: Option<MessageId>) -> ResponseResult<()> {
    let state = crate::models::get_state();
    let (_, _, llm_model) = repos::get_user_profile(&state.db_pool, user_id).await;
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

    crate::services::bazi_service::build_and_save_bazi_html(user_id, &username, &structured_data).await;

    let chart_url = format!("{}/bazi_{}.html", state.config.base_url, user_id);
    let _ = bot
        .send_message(chat_id, format!("📊 <b>Bazi Chart Diagram</b>\n\nView your chart here:\n{}", chart_url))
        .parse_mode(teloxide::types::ParseMode::Html)
        .await;

    match crate::services::bazi_service::core_bazi_analysis(&state, &structured_data, llm_model).await {
        Ok(receiver) => {
            let placeholder = bot.send_message(chat_id, "🔮 Generating bazi analysis...").await;
            let placeholder_msg_id = match placeholder {
                Ok(msg) => msg.id,
                Err(e) => {
                    error!("Failed to send placeholder message: {}", e);
                    return Ok(());
                }
            };

            let reading = super::helpers::stream_to_telegram(&bot, chat_id, placeholder_msg_id, receiver).await;
            if !reading.is_empty() {
                repos::save_user_bazi_analysis(&state.db_pool, user_id, &reading).await;

                {
                    let mut ctx = state.user_contexts.entry(user_id).or_default();
                    ctx.push_message(format!("Assistant: {}", reading), state.config.max_context_messages);
                }
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
    let (user_bazi_four_pillars, bazi_analysis, _llm_model) = repos::get_user_profile(&state.db_pool, user_id).await;

    let Some(user_bazi_four_pillars_raw) = user_bazi_four_pillars.as_deref() else {
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

            let base_url = state.config.base_url.clone();
            let profile_msg = format!(
                "👤 <b>Your Bazi Profile (个人八字档案)</b>\n\n\
                <b>Basic Information (基本信息):</b>\n\
                • <b>Gender (性别):</b> {gender_str}\n\
                • <b>Solar Birthday (阳历生日):</b> {solar_str}\n\
                • <b>Lunar Birthday (农历生日):</b> {lunar_str}\n\n\
                <b>Bazi Four Pillars (四柱排盘):</b>\n\
                {pillars_text}
                <a href=\"{base_url}/bazi_{user_id}.html\">View Bazi Chart Diagram</a>"
            );

            bot.send_message(chat_id, profile_msg).parse_mode(teloxide::types::ParseMode::Html).await?;

            if let Some(reading) = bazi_analysis.filter(|r| !r.is_empty()) {
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
