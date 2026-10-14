//! Service to fetch and format traditional Chinese Almanac (Huangli) data.
use crate::models::common::{BRANCHES, STEMS};
use crate::models::{AppError, AppResult, AppState, LlmResponse};
use crate::services::paipan::bazi_utils::get_empty_death;
use async_openai::types::{chat::ChatCompletionRequestSystemMessageArgs, chat::ChatCompletionRequestUserMessageArgs};
use reqwest::Client;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

pub struct DailyFortuneResult {
  pub almanac_data: String,
  pub response: LlmResponse,
}

async fn immediate_response(stream: bool, message: impl Into<String>) -> LlmResponse {
  let message = message.into();
  if stream {
    let (tx, rx) = tokio::sync::mpsc::channel(1);
    let _ = tx.send(tokio_util::bytes::Bytes::from(message)).await;
    LlmResponse::Stream(crate::models::LlmStream::completed(rx))
  } else {
    LlmResponse::Full(message)
  }
}

/// Generate a Daily Fortune reading correlating the user's Destiny Profile with the day's Almanac.
///
/// Handles Destiny Profile validation, calendar retrieval, CoT prompt building, and
/// conversational context synchronization for interactive sessions.
pub async fn get_daily_fortune(
  state: &Arc<AppState>,
  user_id: u64,
  target_date: &str,
  stream: bool,
  request_type: crate::models::LlmRequestType,
  reasoning_effort: Option<crate::models::ReasoningEffortLevel>,
) -> AppResult<DailyFortuneResult> {
  let user_profile = crate::repos::get_user_profile(&state.db_pool, user_id).await?;
  let bazi_four_pillars =
    user_profile.bazi_four_pillars.as_deref().and_then(|raw| crate::services::paipan::models::StructuredBazi::from_json(Some(raw)).ok()).map(|b| b.to_string()).ok_or(AppError::ProfileNotFound)?;

  let almanac_data = state.almanac_provider.fetch_almanac(target_date).await?;

  let bazi_summary = user_profile.bazi_summary.as_deref().unwrap_or_else(|| user_profile.bazi_analysis.as_deref().unwrap_or("未提供"));

  let system_message = ChatCompletionRequestSystemMessageArgs::default().content(include_str!("../../prompts/BaziHuangLiAssistant.md")).build()?;

  let user_content = format!(
    "请结合以下信息进行精确的日运势推演：\n【用户八字排盘】\n{}\n【命理核心摘要（既有分析）】\n{}\n【目标预测日期】\n{}\n【时辰解释时区（系统默认）】\n{}\n【该日黄历数据】\n{}",
    bazi_four_pillars, bazi_summary, target_date, state.config.app_timezone, almanac_data
  );

  debug!("Built fortune prompt");
  let user_message = ChatCompletionRequestUserMessageArgs::default().content(user_content).build()?;

  let model = crate::repos::resolve_llm_model(&state.db_pool, user_profile.llm_model).await?;
  let model_id = Some(model.id() as i64);
  let model_name = model.as_str().to_string();

  let params = crate::services::llm::LlmRequestParams {
    stream: Some(stream),
    temperature: Some(0.2),
    top_p: Some(0.75),
    user_id: Some(user_id as i64),
    model_id,
    reasoning_effort: reasoning_effort.and_then(|r| r.into_upstream()),
    ..crate::services::llm::LlmRequestParams::new(model_name.clone(), vec![system_message.into(), user_message.into()], request_type)
  };

  let is_interactive = matches!(request_type, crate::models::LlmRequestType::CalendarDate | crate::models::LlmRequestType::CalendarToday | crate::models::LlmRequestType::CalendarTomorrow);
  let almanac_msg = format!("📅 【{} 黄历信息】\n\n{}", target_date, almanac_data);

  if is_interactive {
    // Clear chat history for fresh reading
    {
      let mut ctx = state.user_contexts.entry(user_id).or_default();
      ctx.clear_messages();
      ctx.history_loaded = true;
      ctx.last_active = chrono::Utc::now();
    }
    if let Err(e) = state.chat_cache.clear(user_id).await {
      error!("Failed to clear chat cache for user {}: {}", user_id, e);
    }
  }

  info!("Sending request to LLM (Model: {}, stream: {})...", model_name, stream);
  let response = state.llm_service.call(&state.db_pool, params, &state.runtime).await?;

  let managed_response = if is_interactive {
    match response {
      LlmResponse::Stream(raw_stream) => {
        let state_clone = state.clone();
        let tool_msg = almanac_msg.clone();
        let stream = crate::services::conversation::wrap_persisted_stream(&state.runtime.tasks, raw_stream, move |reply| async move {
          crate::services::conversation::persist_turn_response(&state_clone, user_id, Some(&tool_msg), &reply).await;
        });
        LlmResponse::Stream(stream)
      }
      LlmResponse::Full(reply) => {
        crate::services::conversation::persist_turn_response(state, user_id, Some(&almanac_msg), &reply).await;
        LlmResponse::Full(reply)
      }
    }
  } else {
    response
  };

  Ok(DailyFortuneResult { almanac_data, response: managed_response })
}

pub async fn fetch_and_format_almanac(client: &Client, api_url: &str, target_date: &str) -> crate::models::AppResult<String> {
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

use futures::future::BoxFuture;

/// Abstract provider for traditional Chinese Almanac (Huangli) data.
pub trait AlmanacProvider: Send + Sync + std::fmt::Debug {
  fn fetch_almanac<'a>(&'a self, target_date: &'a str) -> BoxFuture<'a, AppResult<String>>;
}

/// Default HTTP provider fetching almanac data from configured upstream API.
#[derive(Debug, Clone)]
pub struct HttpAlmanacProvider {
  pub client: Client,
  pub api_url: String,
}

impl HttpAlmanacProvider {
  pub fn new(client: Client, api_url: impl Into<String>) -> Self {
    Self { client, api_url: api_url.into() }
  }
}

impl AlmanacProvider for HttpAlmanacProvider {
  fn fetch_almanac<'a>(&'a self, target_date: &'a str) -> BoxFuture<'a, AppResult<String>> {
    Box::pin(fetch_and_format_almanac(&self.client, &self.api_url, target_date))
  }
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

pub struct DateSelectionParams<'a> {
  pub user_id: u64,
  pub start_date: &'a str,
  pub end_date: &'a str,
  pub activity: &'a str,
  pub stream: bool,
  pub request_type: crate::models::LlmRequestType,
  pub reasoning_effort: Option<crate::models::ReasoningEffortLevel>,
}

pub struct DateSelectionResult {
  pub combined_almanac: String,
  pub response: LlmResponse,
}

/// Perform Date Selection (择吉日) across a candidate date window.
///
/// Validates the date window and activity, verifies the user's Destiny Profile,
/// concurrently retrieves almanac records, builds the selection prompt, and
/// synchronizes consultation session state.
pub async fn pick_dates(state: &Arc<AppState>, params: DateSelectionParams<'_>) -> AppResult<DateSelectionResult> {
  // Parse and validate dates
  let start = chrono::NaiveDate::parse_from_str(params.start_date, "%Y-%m-%d").map_err(|_| AppError::Message("start_date must be YYYY-MM-DD format".to_string()))?;
  let end = chrono::NaiveDate::parse_from_str(params.end_date, "%Y-%m-%d").map_err(|_| AppError::Message("end_date must be YYYY-MM-DD format".to_string()))?;

  if params.activity.trim().is_empty() {
    return Err(AppError::Message("activity must not be empty".to_string()));
  }

  let range_days = (end - start).num_days();
  if range_days < 0 {
    return Ok(DateSelectionResult { combined_almanac: String::new(), response: immediate_response(params.stream, "⚠️ 结束日期不能早于开始日期，请重新选择。").await });
  }
  if range_days > 14 {
    return Ok(DateSelectionResult {
      combined_almanac: String::new(), response: immediate_response(params.stream, "⚠️ 选定的日期范围过大。开始与结束日期最多相隔 14 天，请重新选择。").await
    });
  }

  let user_profile = crate::repos::get_user_profile(&state.db_pool, params.user_id).await?;
  let bazi_four_pillars =
    user_profile.bazi_four_pillars.as_deref().and_then(|raw| crate::services::paipan::models::StructuredBazi::from_json(Some(raw)).ok()).map(|b| b.to_string()).ok_or(AppError::ProfileNotFound)?;

  let bazi_summary = user_profile.bazi_summary.as_deref().unwrap_or_else(|| user_profile.bazi_analysis.as_deref().unwrap_or("未提供"));

  let mut current = start;
  let mut dates = Vec::with_capacity((range_days + 1) as usize);
  while current <= end {
    dates.push(current.format("%Y-%m-%d").to_string());
    current = match current.succ_opt() {
      Some(next) => next,
      None => break,
    };
  }

  info!(date_count = dates.len(), "Fetching almanac data for date selection");

  let mut fetch_futures = Vec::new();
  for date_str in &dates {
    let provider = state.almanac_provider.clone();
    fetch_futures.push(async move {
      let res = provider.fetch_almanac(date_str).await;
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
        warn!("Failed to fetch almanac for {}: {}", date_str, e);
        combined_almanac.push_str(&format!("【{}】\n暂无数据。\n\n", date_str));
      }
    }
  }

  if valid_dates == 0 {
    return Ok(DateSelectionResult { combined_almanac: String::new(), response: immediate_response(params.stream, "⚠️ 所选范围内没有可验证的黄历数据，请稍后重试。").await });
  }

  let system_message = ChatCompletionRequestSystemMessageArgs::default().content(include_str!("../../prompts/DateSelectionAssistant.md")).build()?;

  let user_content = format!(
    "请结合以下信息为用户进行择吉日推演：\n【用户八字排盘】\n{}\n【命理核心摘要（既有分析）】\n{}\n【目标活动】\n{}\n【候选日期范围】\n{} 至 {}\n【事件时区（系统默认）】\n{}\n【备选日期范围黄历数据】\n{}",
    bazi_four_pillars, bazi_summary, params.activity, params.start_date, params.end_date, state.config.app_timezone, combined_almanac
  );

  debug!("Built date selection prompt");
  let user_message = ChatCompletionRequestUserMessageArgs::default().content(user_content).build()?;

  let model = crate::repos::resolve_llm_model(&state.db_pool, user_profile.llm_model).await?;
  let model_id = Some(model.id() as i64);
  let model_name = model.as_str().to_string();

  let llm_params = crate::services::llm::LlmRequestParams {
    temperature: Some(0.2),
    top_p: Some(0.75),
    stream: Some(params.stream),
    user_id: Some(params.user_id as i64),
    model_id,
    reasoning_effort: params.reasoning_effort.and_then(|r| r.into_upstream()),
    ..crate::services::llm::LlmRequestParams::new(model_name.clone(), vec![system_message.into(), user_message.into()], params.request_type)
  };

  let is_interactive = matches!(params.request_type, crate::models::LlmRequestType::PickSelection);

  let pick_msg = if is_interactive {
    if !combined_almanac.is_empty() {
      format!("🎯 【择吉日黄历信息】活动: {}, 日期范围: {} 至 {}\n\n{}", params.activity, params.start_date, params.end_date, combined_almanac)
    } else {
      format!("🎯 择吉日：活动「{}」，日期范围：{} 至 {}", params.activity, params.start_date, params.end_date)
    }
  } else {
    String::new()
  };

  if is_interactive {
    // Clear previous context
    {
      let mut ctx = state.user_contexts.entry(params.user_id).or_default();
      ctx.clear_messages();
      ctx.history_loaded = true;
      ctx.last_active = chrono::Utc::now();
    }
    if let Err(e) = state.chat_cache.clear(params.user_id).await {
      error!("Failed to clear chat cache for user {}: {}", params.user_id, e);
    }
  }

  info!("Sending request to LLM for Date Selection (Model: {}, stream: {})...", model_name, params.stream);
  let response = state.llm_service.call(&state.db_pool, llm_params, &state.runtime).await?;

  let managed_response = if is_interactive {
    let tool_msg = if !pick_msg.is_empty() { Some(pick_msg.as_str()) } else { None };
    match response {
      LlmResponse::Stream(raw_stream) => {
        let state_clone = state.clone();
        let user_id = params.user_id;
        let tool_owned = tool_msg.map(str::to_string);
        let stream = crate::services::conversation::wrap_persisted_stream(&state.runtime.tasks, raw_stream, move |reply| async move {
          crate::services::conversation::persist_turn_response(&state_clone, user_id, tool_owned.as_deref(), &reply).await;
        });
        LlmResponse::Stream(stream)
      }
      LlmResponse::Full(reply) => {
        crate::services::conversation::persist_turn_response(state, params.user_id, tool_msg, &reply).await;
        LlmResponse::Full(reply)
      }
    }
  } else {
    response
  };

  Ok(DateSelectionResult { combined_almanac, response: managed_response })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn immediate_response_preserves_stream_contract() {
    match immediate_response(true, "validation failed").await {
      LlmResponse::Stream(mut receiver) => {
        assert_eq!(receiver.recv().await.as_deref(), Some(b"validation failed".as_slice()));
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
