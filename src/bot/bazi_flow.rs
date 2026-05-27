use std::sync::Arc;
use teloxide::{prelude::*, types::MessageId};
use tracing::error;

use crate::models::AppState;
use crate::repos;
use crate::services::{llm_bazi, paipan, solar_time};
use crate::utils;

/// Core logic for Bazi chart calculation and destiny reading generation.
pub async fn perform_bazi_analysis(state: Arc<AppState>, bot: Bot, chat_id: ChatId, user_id: u64, user_display_name: String, message_id: Option<MessageId>) -> ResponseResult<()> {
    let (date, hour, minute, gender, location, location_status) = if let Some(ctx) = state.user_contexts.get(&user_id) {
        let date = ctx.birthdate.clone().unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d").to_string());
        let hour = ctx.hour.unwrap_or(0);
        let minute = ctx.minute.unwrap_or(0);
        let gender = ctx.gender.unwrap_or(1);
        let location = ctx.location.clone();
        let location_status = if let Some(city) = &location {
            format!("✅ City selected: {}", city)
        } else {
            "✅ Using Standard Time (120°E)".to_string()
        };
        (date, hour, minute, gender, location, location_status)
    } else {
        error!("Missing user context data for user_id: {}", user_id);
        let msg = "⚠️ Session expired or missing data. Please start again with /new.";
        if let Some(msg_id) = message_id {
            if let Err(e) = bot.edit_message_text(chat_id, msg_id, msg).await {
                error!("Failed to edit message: {}", e);
            }
        } else {
            if let Err(e) = bot.send_message(chat_id, msg).await {
                error!("Failed to send message: {}", e);
            }
        }
        return Ok(());
    };

    // Let the user know we're working
    let status_text = format!(
        "{}\n✅ Time received.\n\n📅 Date: {}\n🕒 Time: {:02}:{:02}\n⌛ Calculating your Bazi chart...",
        location_status, date, hour, minute
    );

    if let Some(msg_id) = message_id {
        let _ = bot.edit_message_text(chat_id, msg_id, status_text).await;
    } else {
        let _ = bot.send_message(chat_id, status_text).await;
    }

    let _ = bot.send_chat_action(chat_id, teloxide::types::ChatAction::Typing).await;

    // Calculate True Solar Time if location is provided
    let naive_dt = chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d")
        .expect("Pending date should be valid YYYY-MM-DD")
        .and_hms_opt(hour as u32, minute as u32, 0)
        .expect("Hour and minute should be valid (0-23, 0-59)");

    let solar_dt = if let Some(city_name) = &location {
        solar_time::calculate_true_solar_time(naive_dt, city_name, 120.0)
    } else {
        naive_dt
    };

    let birth_year = date.chars().take(4).collect::<String>().parse::<i16>().unwrap_or(1970);
    match paipan::fetch_bazi_chart(&state.http_client, solar_dt, gender, birth_year).await {
        Ok(structured_json) => {
            let birth_dt_str = format!("{} {:02}:{:02}:00", date, hour, minute);
            repos::save_or_update_user_bazi_four_pillars(&state.db_pool, user_id, &structured_json, gender, Some(&birth_dt_str)).await;
            // let formatted_bazi_four_pillars = paipan::format_bazi_for_prompt(&chart);

            // let html_diagram = paipan::generate_bazi_html(&chart, &user_display_name);

            // Save HTML to public folder for web view / Instant View
            let filename = format!("bazi_{}.html", user_id);
            // let public_path = std::path::PathBuf::from("public").join(&filename);

            // if let Err(e) = tokio::fs::write(&public_path, html_diagram).await {
            //     error!("Failed to save Bazi HTML to public: {}", e);
            // }

            let chart_url = format!("{}/{}", state.config.base_url, filename);
            let _ = bot
                .send_message(
                    chat_id,
                    format!(
                        "📊 <b>Bazi Chart Diagram</b>\n\nView your chart here:\n{}\n\n(Tip: If configured, this link supports Instant View)",
                        chart_url
                    ),
                )
                .parse_mode(teloxide::types::ParseMode::Html)
                .await;

            let _ = bot.send_message(chat_id, "✅ Bazi chart calculated!\n🔮 Generating destiny analysis... (this may take a moment)").await;

            match llm_bazi::generate_destiny_reading(&structured_json, &state.config.openai_api_key, &state.config.openai_api_base, &state.config.llm_model_name).await {
                Ok(reading) => {
                    repos::save_user_destiny_reading(&state.db_pool, user_id, &reading).await;

                    let parts = utils::split_message(&reading, 4000);
                    for part in parts {
                        let _ = bot.send_message(chat_id, part).await;
                    }
                }
                Err(e) => {
                    error!("Error generating destiny reading: {}", e);
                    let _ = bot.send_message(chat_id, format!("❌ Error generating analysis: {}", e)).await;
                }
            }
        }
        Err(e) => {
            error!("Failed to fetch bazi chart: {}", e);
            let _ = bot.send_message(chat_id, "❌ Error fetching Bazi chart from API. Please try again later.".to_string()).await;
        }
    }

    Ok(())
}

/// Query and display the user's Bazi profile and destiny reading.
pub async fn display_user_profile(bot: Bot, chat_id: ChatId, user_id: u64, state: Arc<AppState>) -> ResponseResult<()> {
    let (user_profile_bazi_four_pillars, destiny_reading) = repos::get_user_profile(&state.db_pool, user_id).await;

    let Some(user_bazi_four_pillars_raw) = user_profile_bazi_four_pillars.as_deref() else {
        bot.send_message(chat_id, "⚠️ You haven't set your birthdate yet! Please use /new to set your birthdate first to get accurate readings.")
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
                pillars_text.push_str(&format!(
                    "• <b>{name} ({eng_name}):</b> {main_star} {stem}{branch} ({nayin})\n",
                    name = p.name,
                    main_star = main_star,
                    stem = stem,
                    branch = p.base.branch,
                    nayin = p.base.nayin
                ));
            }

            let profile_msg = format!(
                "👤 <b>Your Bazi Profile (个人八字档案)</b>\n\n\
                <b>Basic Information (基本信息):</b>\n\
                • <b>Gender (性别):</b> {gender_str}\n\
                • <b>Solar Birthday (阳历生日):</b> {solar_str}\n\
                • <b>Lunar Birthday (农历生日):</b> {lunar_str}\n\n\
                <b>Bazi Four Pillars (四柱排盘):</b>\n\
                {pillars_text}\n"
            );

            bot.send_message(chat_id, profile_msg).parse_mode(teloxide::types::ParseMode::Html).await?;

            if let Some(reading) = destiny_reading.filter(|r| !r.is_empty()) {
                bot.send_message(chat_id, "🔮 <b>Destiny Reading (命理分析):</b>").parse_mode(teloxide::types::ParseMode::Html).await?;

                let parts = utils::split_message(&reading, 4000);
                for part in parts {
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
