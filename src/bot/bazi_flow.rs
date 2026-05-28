use teloxide::{prelude::*, types::MessageId};
use tracing::error;

use crate::repos;
use crate::services::{paipan, solar_time};
use crate::utils;

/// Core logic for Bazi chart calculation and destiny reading generation.
pub async fn perform_bazi_analysis(bot: Bot, chat_id: ChatId, user_id: u64, user_display_name: String, message_id: Option<MessageId>) -> ResponseResult<()> {
    let state = crate::models::get_state();
    let (birth_date, birth_hour, birth_minute, gender, location, location_status) = if let Some(ctx) = state.user_contexts.get(&user_id) {
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
        location_status, birth_date, birth_hour, birth_minute
    );

    if let Some(msg_id) = message_id {
        let _ = bot.edit_message_text(chat_id, msg_id, status_text).await;
    } else {
        let _ = bot.send_message(chat_id, status_text).await;
    }

    let _ = bot.send_chat_action(chat_id, teloxide::types::ChatAction::Typing).await;

    // Calculate True Solar Time if location is provided
    let naive_dt = chrono::NaiveDate::parse_from_str(&birth_date, "%Y-%m-%d")
        .expect("Pending date should be valid YYYY-MM-DD")
        .and_hms_opt(birth_hour as u32, birth_minute as u32, 0)
        .expect("Hour and minute should be valid (0-23, 0-59)");

    let solar_dt = if let Some(city_name) = &location {
        solar_time::calculate_true_solar_time(naive_dt, city_name, 120.0)
    } else {
        naive_dt
    };

    let birth_year = birth_date.chars().take(4).collect::<String>().parse::<i32>().unwrap_or(1970);
    match paipan::fetch_bazi_chart(&state.http_client, solar_dt, gender, birth_year).await {
        Ok((structured_data, structured_json)) => {
            repos::upsert_user_bazi(&state.db_pool, user_id, &structured_json, gender, &birth_date).await;

            let html_diagram = paipan::generate_bazi_html(&structured_data, &user_display_name);
            // Save HTML to public folder for web view / Instant View
            let filename = format!("bazi_{}.html", user_id);
            let public_path = std::path::PathBuf::from("public").join(&filename);
            if let Err(e) = tokio::fs::write(&public_path, html_diagram).await {
                error!("Failed to save Bazi HTML to public: {}", e);
            }
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

            let _ = bot.send_message(chat_id, "🔮 Now generating bazi analysis... (this may take a moment)").await;

            let system_prompt = include_str!("../../prompts/UserBazi.md");
            let full_user_content = format!("【待分析命盘】 [Bazi Info]\n{}", structured_json);
            let system_message = async_openai::types::chat::ChatCompletionRequestSystemMessageArgs::default().content(system_prompt).build().unwrap();
            let user_message = async_openai::types::chat::ChatCompletionRequestUserMessageArgs::default().content(full_user_content).build().unwrap();

            let mut params = crate::services::llm::LlmRequestParams::new(state.config.llm_model_name.clone(), vec![system_message.into(), user_message.into()]);
            params.frequency_penalty = Some(0.5);
            params.presence_penalty = Some(0.5);
            params.temperature = Some(0.2);
            params.top_p = Some(0.75);

            match crate::services::llm::call_llm(&state.db_pool, &state.config.llm_client_config, params).await {
                Ok(response) => {
                    if let Some(reading) = response.choices.first().and_then(|c| c.message.content.clone()) {
                        repos::save_user_bazi_analysis(&state.db_pool, user_id, &reading).await;

                        let parts = utils::split_message(&reading, 4000);
                        for part in parts {
                            let _ = bot.send_message(chat_id, part).await;
                        }
                    } else {
                        let _ = bot.send_message(chat_id, "❌ Error: No valid content in LLM response").await;
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
pub async fn display_user_profile(bot: Bot, chat_id: ChatId, user_id: u64) -> ResponseResult<()> {
    let state = crate::models::get_state();
    let (user_bazi_four_pillars, bazi_analysis) = repos::get_user_profile(&state.db_pool, user_id).await;

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
                {pillars_text}\n
                <a href=\"{base_url}/bazi_{user_id}.html\">View Bazi Chart Diagram</a>"
            );

            bot.send_message(chat_id, profile_msg).parse_mode(teloxide::types::ParseMode::Html).await?;

            if let Some(reading) = bazi_analysis.filter(|r| !r.is_empty()) {
                bot.send_message(chat_id, "🔮 <b>Bazi Analysis (命理分析):</b>").parse_mode(teloxide::types::ParseMode::Html).await?;

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
