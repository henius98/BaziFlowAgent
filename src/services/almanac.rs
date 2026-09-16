//! Service to fetch and format traditional Chinese Almanac (Huangli) data.
use crate::models::common::{BRANCHES, STEMS};
use crate::models::{AppError, AppResult, LlmResponse, LogErrorExt};
use crate::services::paipan::bazi_utils::get_empty_death;
use async_openai::types::{chat::ChatCompletionRequestSystemMessageArgs, chat::ChatCompletionRequestUserMessageArgs};
use reqwest::Client;
use tracing::{debug, info};

pub struct DateFortuneRequest<'a> {
  pub target_date: &'a str,
  pub almanac_data: &'a str,
  pub bazi_four_pillars: &'a str,
  pub bazi_summary: &'a str,
  /// Whether to stream the LLM response. Defaults to `false`.
  pub stream: bool,
  pub llm_model: Option<crate::models::common::LlmModel>,
  pub user_id: Option<i64>,
  pub request_type: Option<crate::models::LlmRequestType>,
}

async fn immediate_response(stream: bool, message: impl Into<String>) -> LlmResponse {
  let message = message.into();
  if stream {
    let (tx, rx) = tokio::sync::mpsc::channel(1);
    let _ = tx.send(message).await;
    LlmResponse::Stream(crate::models::LlmStream::completed(rx))
  } else {
    LlmResponse::Full(message)
  }
}

/// Analyze date fortune using the almanac + LLM.
/// When `req.stream` is true, returns `LlmResponse::Stream`; otherwise `LlmResponse::Full`.
pub async fn analysis_date_fortune(req: DateFortuneRequest<'_>) -> AppResult<LlmResponse> {
  let stream = req.stream;
  let state = crate::models::get_state();

  let almanac_data = req.almanac_data;
  debug!("Building LLM prompt with provided almanac data...");

  let system_message = ChatCompletionRequestSystemMessageArgs::default().content(include_str!("../../prompts/BaziHuangLiAssistant.md")).build()?;

  if req.bazi_four_pillars.trim().is_empty() {
    let msg = "请先输入您的生辰八字进行排盘。".to_string();
    if stream {
      let (tx, rx) = tokio::sync::mpsc::channel(1);
      let _ = tx.send(msg).await;
      return Ok(LlmResponse::Stream(crate::models::LlmStream::completed(rx)));
    }
    return Ok(LlmResponse::Full(msg));
  }

  let bazi_summary = if req.bazi_summary.trim().is_empty() { "未提供" } else { req.bazi_summary };
  let user_content = format!(
    "请结合以下信息进行精确的日运势推演：\n【用户八字排盘】\n{}\n【命理核心摘要（既有分析）】\n{}\n【目标预测日期】\n{}\n【时辰解释时区（系统默认）】\n{}\n【该日黄历数据】\n{}",
    req.bazi_four_pillars, bazi_summary, req.target_date, state.config.app_timezone, almanac_data
  );

  debug!("Built fortune prompt");
  let user_message = ChatCompletionRequestUserMessageArgs::default().content(user_content).build()?;

  let model_name = req.llm_model.map(|m| m.as_str().to_string()).unwrap_or_else(|| state.config.llm_model_name.clone());
  let mut params = crate::services::llm::LlmRequestParams::new(model_name.clone(), vec![system_message.into(), user_message.into()]);
  params.temperature = Some(0.2);
  params.top_p = Some(0.75);

  params.stream = Some(stream);
  params.user_id = req.user_id;
  params.request_type = req.request_type;

  info!("Sending request to LLM (Model: {}, stream: {})...", model_name, stream);
  state.llm_service.call(params).await
}

pub async fn fetch_and_format_almanac(client: &Client, target_date: &str) -> crate::models::AppResult<String> {
  let state = crate::models::get_state();
  let api_url = &state.config.upstreams.almanac;

  let response = client.get(api_url).query(&[("date", target_date)]).send().await?.error_for_status()?;

  // Fetch bounded bytes first, then fail closed if the upstream schema is invalid.
  let text_response = crate::services::http::body(response).await?;

  let data = serde_json::from_slice::<AlmanacResponse>(&text_response).map_err(|e| {
    tracing::warn!("Failed to deserialize almanac data: {}", e);
    AppError::Message("黄历数据格式无效，请稍后重试。".to_string())
  })?;
  validate_almanac_data(&data, target_date)?;
  Ok(format_almanac_data(&data))
}

use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct AlmanacResponse {
  pub solar: Option<SolarData>,
  pub lunar: Option<LunarData>,
  #[serde(rename = "ganZhi")]
  pub gan_zhi: Option<GanZhiData>,
  #[serde(rename = "yiJi")]
  pub yi_ji: Option<YiJiData>,
  pub hours: Option<Vec<HourData>>,
  pub positions: Option<PositionsData>,
  pub info: Option<InfoData>,
  pub bottom: Option<BottomData>,
}

#[derive(Deserialize, Debug)]
pub struct SolarData {
  pub year: i32,
  pub month: u32,
  pub day: u32,
  #[serde(rename = "weekInChinese")]
  pub week_in_chinese: Option<String>,
  #[serde(rename = "xingZuo")]
  pub xing_zuo: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct HourData {
  #[serde(rename = "ganZhi")]
  pub gan_zhi: Option<String>,
  pub luck: Option<String>,
  pub zhi: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct YiJiData {
  pub yi: Option<Vec<String>>,
  pub ji: Option<Vec<String>>,
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

fn validate_almanac_data(data: &AlmanacResponse, target_date: &str) -> AppResult<()> {
  let expected_date = chrono::NaiveDate::parse_from_str(target_date, "%Y-%m-%d").map_err(|_| AppError::Message("目标日期格式无效。".to_string()))?;
  let solar = data.solar.as_ref().ok_or_else(|| AppError::Message("黄历响应缺少公历日期。".to_string()))?;
  let source_date = chrono::NaiveDate::from_ymd_opt(solar.year, solar.month, solar.day).ok_or_else(|| AppError::Message("黄历响应包含无效公历日期。".to_string()))?;
  if source_date != expected_date {
    return Err(AppError::Message(format!("黄历响应日期不匹配：请求 {}，收到 {}。", expected_date, source_date)));
  }

  let day_gan_zhi = data.gan_zhi.as_ref().and_then(|gan_zhi| gan_zhi.day.as_deref()).map(str::trim).filter(|value| !value.is_empty());
  let Some(day_gan_zhi) = day_gan_zhi else {
    return Err(AppError::Message("黄历响应缺少日干支。".to_string()));
  };
  let mut chars = day_gan_zhi.chars();
  let stem = chars.next().map(|value| value.to_string());
  let branch = chars.next().map(|value| value.to_string());
  let valid = chars.next().is_none() && stem.as_deref().is_some_and(|value| STEMS.contains(&value)) && branch.as_deref().is_some_and(|value| BRANCHES.contains(&value));
  if !valid {
    return Err(AppError::Message("黄历响应包含无效日干支。".to_string()));
  }

  Ok(())
}

const STANDARD_HOUR_BRANCHES: [&str; 13] = ["子", "丑", "寅", "卯", "辰", "巳", "午", "未", "申", "酉", "戌", "亥", "子"];
const STANDARD_HOUR_RANGES: [&str; 13] =
  ["00:00–00:59", "01:00–02:59", "03:00–04:59", "05:00–06:59", "07:00–08:59", "09:00–10:59", "11:00–12:59", "13:00–14:59", "15:00–16:59", "17:00–18:59", "19:00–20:59", "21:00–22:59", "23:00–23:59"];

fn has_standard_hour_order(hours: &[HourData]) -> bool {
  hours.len() == STANDARD_HOUR_BRANCHES.len() && hours.iter().zip(STANDARD_HOUR_BRANCHES).all(|(hour, expected)| hour.zhi.as_deref() == Some(expected))
}

/// Formats the statically-typed API response into clean text.
fn format_almanac_data(data: &AlmanacResponse) -> String {
  let mut parts = Vec::new();

  if let Some(solar) = &data.solar {
    let mut solar_parts = vec![format!("公历: {:04}-{:02}-{:02}", solar.year, solar.month, solar.day)];
    if let Some(week) = &solar.week_in_chinese {
      solar_parts.push(format!("星期{}", week));
    }
    if let Some(xing_zuo) = &solar.xing_zuo {
      solar_parts.push(format!("星座: {}", xing_zuo));
    }
    parts.push(solar_parts.join(", "));
  }

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

  if let Some(yi_ji) = &data.yi_ji {
    let mut yi_ji_parts = Vec::new();
    if let Some(yi) = &yi_ji.yi
      && !yi.is_empty()
    {
      yi_ji_parts.push(format!("宜: {}", yi.join("、")));
    }
    if let Some(ji) = &yi_ji.ji
      && !ji.is_empty()
    {
      yi_ji_parts.push(format!("忌: {}", ji.join("、")));
    }
    if !yi_ji_parts.is_empty() {
      parts.push(format!("宜忌:\n  {}", yi_ji_parts.join("\n  ")));
    }
  }

  if let Some(hours) = &data.hours {
    let mut hour_parts = Vec::new();
    let standard_order = has_standard_hour_order(hours);
    for (index, h) in hours.iter().enumerate() {
      let mut fields = Vec::new();
      if standard_order {
        fields.push(format!("时段: {}", STANDARD_HOUR_RANGES[index]));
      }
      if let Some(zhi) = &h.zhi {
        fields.push(format!("时支: {}", zhi));
      }
      if let Some(gz) = &h.gan_zhi {
        fields.push(format!("干支: {}", gz));
      }
      if let Some(luck) = &h.luck {
        fields.push(format!("吉凶: {}", luck));
      }
      if !fields.is_empty() {
        hour_parts.push(format!("记录{} [{}]", index + 1, fields.join(", ")));
      }
    }
    if !hour_parts.is_empty() {
      parts.push(format!("时辰:\n  {}", hour_parts.join("\n  ")));
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
      b_parts.push(format!("凶煞: {}", v.join(" ")));
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
  pub bazi_summary: &'a str,
  pub stream: bool,
  pub llm_model: Option<crate::models::common::LlmModel>,
  pub user_id: Option<i64>,
  pub request_type: Option<crate::models::LlmRequestType>,
}

pub async fn analysis_pick_selection(req: PickSelectionRequest<'_>) -> AppResult<LlmResponse> {
  let stream = req.stream;
  let state = crate::models::get_state();

  // Parse dates
  let start = chrono::NaiveDate::parse_from_str(req.start_date, "%Y-%m-%d").log_err_msg("Invalid start date")?;
  let end = chrono::NaiveDate::parse_from_str(req.end_date, "%Y-%m-%d").log_err_msg("Invalid end date")?;

  let range_days = (end - start).num_days();
  if range_days < 0 {
    return Ok(immediate_response(stream, "⚠️ 结束日期不能早于开始日期，请重新选择。").await);
  }
  if range_days > 14 {
    return Ok(immediate_response(stream, "⚠️ 选定的日期范围过大。开始与结束日期最多相隔 14 天，请重新选择。").await);
  }

  let mut current = start;
  let mut dates = Vec::with_capacity((range_days + 1) as usize);
  while current <= end {
    dates.push(current.format("%Y-%m-%d").to_string());
    current = match current.succ_opt() {
      Some(next) => next,
      None => break,
    };
  }

  tracing::info!(date_count = dates.len(), "Fetching almanac data");

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
  let mut valid_dates = 0usize;

  for (date_str, res) in results {
    match res {
      Ok(data) => {
        valid_dates += 1;
        combined_almanac.push_str(&format!("【{}】\n{}\n\n", date_str, data));
      }
      Err(e) => {
        tracing::warn!("Failed to fetch almanac for {}: {}", date_str, e);
        combined_almanac.push_str(&format!("【{}】\n暂无数据。\n\n", date_str));
      }
    }
  }

  if valid_dates == 0 {
    return Ok(immediate_response(stream, "⚠️ 所选范围内没有可验证的黄历数据，请稍后重试。").await);
  }

  let system_message = ChatCompletionRequestSystemMessageArgs::default().content(include_str!("../../prompts/DateSelectionAssistant.md")).build()?;

  if req.bazi_four_pillars.trim().is_empty() {
    let msg = "请先输入您的生辰八字进行排盘。".to_string();
    if stream {
      let (tx, rx) = tokio::sync::mpsc::channel(1);
      let _ = tx.send(msg).await;
      return Ok(LlmResponse::Stream(crate::models::LlmStream::completed(rx)));
    }
    return Ok(LlmResponse::Full(msg));
  }

  let bazi_summary = if req.bazi_summary.trim().is_empty() { "未提供" } else { req.bazi_summary };
  let user_content = format!(
    "请结合以下信息为用户进行择吉日推演：\n【用户八字排盘】\n{}\n【命理核心摘要（既有分析）】\n{}\n【目标活动】\n{}\n【候选日期范围】\n{} 至 {}\n【事件时区（系统默认）】\n{}\n【备选日期范围黄历数据】\n{}",
    req.bazi_four_pillars, bazi_summary, req.activity, req.start_date, req.end_date, state.config.app_timezone, combined_almanac
  );

  tracing::debug!("Built date selection prompt");
  let user_message = ChatCompletionRequestUserMessageArgs::default().content(user_content).build()?;

  let model_name = req.llm_model.map(|m| m.as_str().to_string()).unwrap_or_else(|| state.config.llm_model_name.clone());
  let mut params = crate::services::llm::LlmRequestParams::new(model_name.clone(), vec![system_message.into(), user_message.into()]);
  params.temperature = Some(0.2); // Low temperature for factual analysis
  params.top_p = Some(0.75);
  params.stream = Some(stream);
  params.user_id = req.user_id;
  params.request_type = req.request_type;

  tracing::info!("Sending request to LLM for Date Selection (Model: {}, stream: {})...", model_name, stream);
  state.llm_service.call(params).await
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn immediate_response_preserves_stream_contract() {
    match immediate_response(true, "validation failed").await {
      LlmResponse::Stream(mut receiver) => {
        assert_eq!(receiver.recv().await.as_deref(), Some("validation failed"));
      }
      LlmResponse::Full(_) => panic!("stream request must not return a full response"),
    }
  }

  #[test]
  fn formats_activity_rules_and_hour_luck_from_current_schema() {
    let raw = r#"{
            "solar": { "year": 2026, "month": 8, "day": 20 },
            "ganZhi": { "day": "丙午" },
            "yiJi": { "yi": ["开市"], "ji": ["嫁娶"] },
            "hours": [
                { "ganZhi": "戊子", "luck": "吉", "zhi": "子" },
                { "ganZhi": "己丑", "luck": "吉", "zhi": "丑" },
                { "ganZhi": "庚寅", "luck": "凶", "zhi": "寅" },
                { "ganZhi": "辛卯", "luck": "凶", "zhi": "卯" },
                { "ganZhi": "壬辰", "luck": "吉", "zhi": "辰" },
                { "ganZhi": "癸巳", "luck": "吉", "zhi": "巳" },
                { "ganZhi": "甲午", "luck": "凶", "zhi": "午" },
                { "ganZhi": "乙未", "luck": "吉", "zhi": "未" },
                { "ganZhi": "丙申", "luck": "凶", "zhi": "申" },
                { "ganZhi": "丁酉", "luck": "凶", "zhi": "酉" },
                { "ganZhi": "戊戌", "luck": "吉", "zhi": "戌" },
                { "ganZhi": "己亥", "luck": "凶", "zhi": "亥" },
                { "ganZhi": "庚子", "luck": "吉", "zhi": "子" }
            ]
        }"#;
    let data: AlmanacResponse = serde_json::from_str(raw).expect("current almanac schema should deserialize");

    let formatted = format_almanac_data(&data);

    validate_almanac_data(&data, "2026-08-20").expect("matching source date and day pillar should validate");
    assert!(formatted.contains("宜: 开市"));
    assert!(formatted.contains("忌: 嫁娶"));
    assert!(formatted.contains("记录1 [时段: 00:00–00:59, 时支: 子, 干支: 戊子, 吉凶: 吉]"));
    assert!(formatted.contains("记录13 [时段: 23:00–23:59, 时支: 子, 干支: 庚子, 吉凶: 吉]"));
    assert!(validate_almanac_data(&data, "2026-08-21").is_err());

    let malformed: AlmanacResponse = serde_json::from_str(r#"{"solar":{"year":2026,"month":8,"day":20},"ganZhi":{"day":"foo"}}"#).expect("malformed day pillar fixture should still deserialize");
    assert!(validate_almanac_data(&malformed, "2026-08-20").is_err());
  }
}
