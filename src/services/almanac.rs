//! Service to fetch and format traditional Chinese Almanac (Huangli) data.
use crate::models::{AppResult, LlmResponse, LogErrorExt};
use crate::services::paipan::bazi_utils::get_empty_death;
use async_openai::types::{chat::ChatCompletionRequestSystemMessageArgs, chat::ChatCompletionRequestUserMessageArgs};
use reqwest::Client;
use serde_json::Value;
use tracing::{debug, info};

pub struct DateFortuneRequest<'a> {
    pub target_date: &'a str,
    pub bazi_four_pillars: &'a str,
    pub bazi_analysis: &'a str,
    /// Whether to stream the LLM response. Defaults to `false`.
    pub stream: bool,
    pub llm_model: Option<crate::models::common::LlmModel>,
    pub user_id: Option<i64>,
    pub request_type: Option<String>,
}

/// Analyze date fortune using the almanac + LLM.
/// When `req.stream` is true, returns `LlmResponse::Stream`; otherwise `LlmResponse::Full`.
pub async fn analysis_date_fortune(req: DateFortuneRequest<'_>) -> AppResult<LlmResponse> {
    let stream = req.stream;
    let state = crate::models::get_state();
    info!("Fetching almanac data for {}", req.target_date);

    let almanac_data = fetch_and_format_almanac(&state.http_client, req.target_date)
        .await
        .log_err_msg("Failed to fetch or format almanac data")?;
    debug!("Almanac data fetched successfully. Building LLM prompt...");

    let system_message = ChatCompletionRequestSystemMessageArgs::default()
        .content(include_str!("../../prompts/BaziHuangLiAssistant.md"))
        .build()?;

    if req.bazi_four_pillars.is_empty() || req.bazi_analysis.is_empty() {
        let msg = "请先输入您的生辰八字进行排盘。".to_string();
        if stream {
            let (tx, rx) = tokio::sync::mpsc::channel(1);
            let _ = tx.send(msg).await;
            return Ok(LlmResponse::Stream(rx));
        }
        return Ok(LlmResponse::Full(msg));
    }

    let user_content = format!(
        "请结合以下信息进行精确的日运势推演：\n【用户八字排盘】\n{}\n【用户命格详批】\n{}\n【目标预测日期】\n{}",
        req.bazi_four_pillars, req.bazi_analysis, almanac_data
    );

    debug!("Full User Prompt:\n{}", user_content);
    let user_message = ChatCompletionRequestUserMessageArgs::default().content(user_content).build()?;

    let model_name = req.llm_model.map(|m| m.as_str().to_string()).unwrap_or_else(|| state.config.llm_model_name.clone());
    let mut params = crate::services::llm::LlmRequestParams::new(model_name.clone(), vec![system_message.into(), user_message.into()]);
    params.frequency_penalty = Some(0.5);
    params.presence_penalty = Some(0.5);
    params.temperature = Some(0.2);
    params.top_p = Some(0.75);

    params.stream = Some(stream);
    params.user_id = req.user_id;
    params.request_type = req.request_type;

    info!("Sending request to LLM (Model: {}, stream: {})...", model_name, stream);
    crate::services::llm::call_llm(&state.db_pool, &state.config.llm_client_config, params).await
}

pub async fn fetch_and_format_almanac(client: &Client, target_date: &str) -> crate::models::AppResult<String> {
    let api_url = format!("https://www.mingdecode.com/api/almanac?date={}", target_date);

    let response = client.get(&api_url).send().await?.error_for_status()?;

    // Fetch as text first so we can parse into Struct, and fallback on failure
    let text_response = response.text().await?;

    match serde_json::from_str::<AlmanacResponse>(&text_response) {
        Ok(data) => Ok(format_almanac_data(&data)),
        Err(e) => {
            tracing::warn!("Failed to deserialize almanac data into struct, error: {}. Falling back to generic text formatting.", e);
            // Fallback: Just parse as generic Value and print
            let raw: Value = serde_json::from_str(&text_response)?;
            Ok(format!("{:#?}", raw))
        }
    }
}

use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct AlmanacResponse {
    pub lunar: Option<LunarData>,
    #[serde(rename = "ganZhi")]
    pub gan_zhi: Option<GanZhiData>,
    pub hours: Option<Vec<HourData>>,
    pub positions: Option<PositionsData>,
    pub info: Option<InfoData>,
    pub bottom: Option<BottomData>,
}

#[derive(Deserialize, Debug)]
pub struct HourData {
    #[serde(rename = "ganZhi")]
    pub gan_zhi: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct PositionsData {
    pub cai: Option<String>,
    pub xi: Option<String>,
    pub fu: Option<String>,
    #[serde(rename = "yangGui")]
    pub yang_gui: Option<String>,
    #[serde(rename = "yinGui")]
    pub yin_gui: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct LunarData {
    #[serde(rename = "monthInChinese")]
    pub month_in_chinese: Option<String>,
    #[serde(rename = "dayInChinese")]
    pub day_in_chinese: Option<String>,
    #[serde(rename = "yearNaYin")]
    pub year_na_yin: Option<String>,
    #[serde(rename = "monthNaYin")]
    pub month_na_yin: Option<String>,
    #[serde(rename = "dayNaYin")]
    pub day_na_yin: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct GanZhiData {
    pub year: Option<String>,
    pub month: Option<String>,
    pub day: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct InfoData {
    #[serde(rename = "chongDesc")]
    pub chong_desc: Option<String>,
    #[serde(rename = "chongShengXiao")]
    pub chong_sheng_xiao: Option<String>,
    pub sha: Option<String>,
    #[serde(rename = "tianShen")]
    pub tian_shen: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct BottomData {
    #[serde(rename = "jiShen")]
    pub ji_shen: Option<Vec<String>>,
    #[serde(rename = "taiShen")]
    pub tai_shen: Option<String>,
    pub xiu: Option<String>,
    #[serde(rename = "xiuLuck")]
    pub xiu_luck: Option<String>,
    #[serde(rename = "zhiXing")]
    pub zhi_xing: Option<String>,
    #[serde(rename = "liuYao")]
    pub liu_yao: Option<String>,
    #[serde(rename = "yueXiang")]
    pub yue_xiang: Option<String>,
    #[serde(rename = "xiongSha")]
    pub xiong_sha: Option<Vec<String>>,
}

/// Formats the statically-typed API response into clean text.
fn format_almanac_data(data: &AlmanacResponse) -> String {
    let mut parts = Vec::new();

    if let Some(lunar) = &data.lunar {
        let mut lunar_parts = Vec::new();
        if let Some(v) = &lunar.month_in_chinese {
            lunar_parts.push(format!("农历月: {}", v));
        }
        if let Some(v) = &lunar.day_in_chinese {
            lunar_parts.push(format!("农历日: {}", v));
        }
        if let Some(v) = &lunar.year_na_yin {
            lunar_parts.push(format!("年纳音: {}", v));
        }
        if let Some(v) = &lunar.month_na_yin {
            lunar_parts.push(format!("月纳音: {}", v));
        }
        if let Some(v) = &lunar.day_na_yin {
            lunar_parts.push(format!("日纳音: {}", v));
        }
        if !lunar_parts.is_empty() {
            parts.push(format!("农历:\n  {}", lunar_parts.join(", ")));
        }
    }

    if let Some(gan_zhi) = &data.gan_zhi {
        let mut gz_parts = Vec::new();
        if let Some(v) = &gan_zhi.year {
            gz_parts.push(format!("年: {}", v));
        }
        if let Some(v) = &gan_zhi.month {
            gz_parts.push(format!("月: {}", v));
        }
        if let Some(v) = &gan_zhi.day {
            gz_parts.push(format!("日: {}", v));
        }
        if !gz_parts.is_empty() {
            parts.push(format!("干支:\n  {}", gz_parts.join(", ")));
        }
    }

    if let Some(hours) = &data.hours {
        let mut hour_parts = Vec::new();
        for h in hours {
            if let Some(gz) = &h.gan_zhi {
                hour_parts.push(gz.clone());
            }
        }
        if !hour_parts.is_empty() {
            parts.push(format!("时辰干支:\n  {}", hour_parts.join(", ")));
        }
    }

    if let Some(pos) = &data.positions {
        let mut pos_parts = Vec::new();
        if let Some(v) = &pos.cai {
            pos_parts.push(format!("财神: {}", v));
        }
        if let Some(v) = &pos.xi {
            pos_parts.push(format!("喜神: {}", v));
        }
        if let Some(v) = &pos.fu {
            pos_parts.push(format!("福神: {}", v));
        }
        if let Some(v) = &pos.yang_gui {
            pos_parts.push(format!("阳贵人: {}", v));
        }
        if let Some(v) = &pos.yin_gui {
            pos_parts.push(format!("阴贵人: {}", v));
        }
        if !pos_parts.is_empty() {
            parts.push(format!("方位:\n  {}", pos_parts.join(", ")));
        }
    }

    // Calculate Kong Wang
    if let Some(gan_zhi) = &data.gan_zhi
        && let Some(day_gz) = &gan_zhi.day
    {
        let stem = day_gz.chars().next().map(|c| c.to_string()).unwrap_or_default();
        let branch = day_gz.chars().nth(1).map(|c| c.to_string()).unwrap_or_default();
        if !stem.is_empty() && !branch.is_empty() {
            let kw = get_empty_death(&stem, &branch);
            parts.push(format!("空亡:\n  {}", kw));
        }
    }

    if let Some(info) = &data.info {
        let mut info_parts = Vec::new();
        if let Some(v) = &info.chong_desc {
            info_parts.push(format!("冲煞: {}", v));
        }
        if let Some(v) = &info.chong_sheng_xiao {
            info_parts.push(format!("冲生肖: {}", v));
        }
        if let Some(v) = &info.sha {
            info_parts.push(format!("煞方: {}", v));
        }
        if let Some(v) = &info.tian_shen {
            info_parts.push(format!("值神: {}", v));
        }
        if !info_parts.is_empty() {
            parts.push(format!("基本信息:\n  {}", info_parts.join(", ")));
        }
    }

    if let Some(bottom) = &data.bottom {
        let mut b_parts = Vec::new();
        if let Some(v) = &bottom.ji_shen {
            b_parts.push(format!("吉神宜趋: {}", v.join(" ")));
        }
        if let Some(v) = &bottom.tai_shen {
            b_parts.push(format!("胎神占方: {}", v));
        }
        if let Some(v) = &bottom.xiu {
            b_parts.push(format!("二十八星宿: {}", v));
        }
        if let Some(v) = &bottom.xiu_luck {
            b_parts.push(format!("星宿吉凶: {}", v));
        }
        if let Some(v) = &bottom.zhi_xing {
            b_parts.push(format!("建除十二神: {}", v));
        }
        if let Some(v) = &bottom.liu_yao {
            b_parts.push(format!("六曜: {}", v));
        }
        if let Some(v) = &bottom.yue_xiang {
            b_parts.push(format!("月相: {}", v));
        }
        if let Some(v) = &bottom.xiong_sha {
            b_parts.push(format!("凶煞宜忌: {}", v.join(" ")));
        }
        if !b_parts.is_empty() {
            parts.push(format!("额外补充:\n  {}", b_parts.join(", ")));
        }
    }

    parts.join("\n")
}

// ─────────────────────────────────────────────
// Date Selection (/pick) Feature
// ─────────────────────────────────────────────

pub struct PickSelectionRequest<'a> {
    pub start_date: &'a str,
    pub end_date: &'a str,
    pub activity: &'a str,
    pub bazi_four_pillars: &'a str,
    pub bazi_analysis: &'a str,
    pub stream: bool,
    pub llm_model: Option<crate::models::common::LlmModel>,
    pub user_id: Option<i64>,
    pub request_type: Option<String>,
}

pub async fn analysis_pick_selection(req: PickSelectionRequest<'_>) -> AppResult<LlmResponse> {
    let stream = req.stream;
    let state = crate::models::get_state();

    // Parse dates
    let start = chrono::NaiveDate::parse_from_str(req.start_date, "%Y-%m-%d").log_err_msg("Invalid start date")?;
    let end = chrono::NaiveDate::parse_from_str(req.end_date, "%Y-%m-%d").log_err_msg("Invalid end date")?;

    let mut current = start;
    let mut dates = Vec::new();
    while current <= end {
        dates.push(current.format("%Y-%m-%d").to_string());
        current = match current.succ_opt() {
            Some(next) => next,
            None => break,
        };
        if current > end {
            break;
        }
    }

    if dates.len() > 15 {
        // Fallback protection
        return Ok(LlmResponse::Full("⚠️ 选定的日期范围过大。为保证推算精度，最多允许选择 14 天的范围。请重新选择。".to_string()));
    }

    tracing::info!("Fetching almanac data for {} dates for activity: {}", dates.len(), req.activity);

    // Fetch almanac for all dates concurrently
    let mut fetch_futures = Vec::new();
    for date_str in &dates {
        let client = &state.http_client;
        fetch_futures.push(async move {
            let res = fetch_and_format_almanac(client, date_str).await;
            (date_str.clone(), res)
        });
    }

    let results = futures::future::join_all(fetch_futures).await;
    let mut combined_almanac = String::new();

    for (date_str, res) in results {
        match res {
            Ok(data) => {
                combined_almanac.push_str(&format!("【{}】\n{}\n\n", date_str, data));
            }
            Err(e) => {
                tracing::warn!("Failed to fetch almanac for {}: {}", date_str, e);
                combined_almanac.push_str(&format!("【{}】\n暂无数据。\n\n", date_str));
            }
        }
    }

    let system_message = ChatCompletionRequestSystemMessageArgs::default()
        .content(include_str!("../../prompts/DateSelectionAssistant.md"))
        .build()?;

    if req.bazi_four_pillars.is_empty() || req.bazi_analysis.is_empty() {
        let msg = "请先输入您的生辰八字进行排盘。".to_string();
        if stream {
            let (tx, rx) = tokio::sync::mpsc::channel(1);
            let _ = tx.send(msg).await;
            return Ok(LlmResponse::Stream(rx));
        }
        return Ok(LlmResponse::Full(msg));
    }

    let user_content = format!(
        "请结合以下信息为用户进行择吉日推演：\n【用户八字排盘】\n{}\n【用户命格详批】\n{}\n【目标活动】\n{}\n【备选日期范围黄历数据】\n{}",
        req.bazi_four_pillars, req.bazi_analysis, req.activity, combined_almanac
    );

    tracing::debug!("Date Selection Full User Prompt:\n{}", user_content);
    let user_message = ChatCompletionRequestUserMessageArgs::default().content(user_content).build()?;

    let model_name = req.llm_model.map(|m| m.as_str().to_string()).unwrap_or_else(|| state.config.llm_model_name.clone());
    let mut params = crate::services::llm::LlmRequestParams::new(model_name.clone(), vec![system_message.into(), user_message.into()]);
    params.frequency_penalty = Some(0.5);
    params.presence_penalty = Some(0.5);
    params.temperature = Some(0.2); // Low temperature for factual analysis
    params.top_p = Some(0.75);
    params.stream = Some(stream);
    params.user_id = req.user_id;
    params.request_type = req.request_type;

    tracing::info!("Sending request to LLM for Date Selection (Model: {}, stream: {})...", model_name, stream);
    crate::services::llm::call_llm(&state.db_pool, &state.config.llm_client_config, params).await
}
