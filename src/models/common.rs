/// Represents a city and its geographical data.
/// A list of common cities for selection.
// Bazi records
/// Five Element (五行) type for the overcoming/destroying (克) cycle
// Calendar
pub use baziflow_core::common::*;

/// LLM response: either a complete string or a streaming channel.
pub enum LlmResponse {
  Full(String),
  Stream(LlmStream),
}

/// Structured chat turn role for conversation history and write-behind cache representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
  User,
  Assistant,
  Tool,
}

impl ChatRole {
  pub fn as_str(&self) -> &'static str {
    match self {
      Self::User => "user",
      Self::Assistant => "assistant",
      Self::Tool => "tool",
    }
  }
}

impl std::str::FromStr for ChatRole {
  type Err = std::convert::Infallible;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    Ok(match s {
      "assistant" => Self::Assistant,
      "tool" => Self::Tool,
      _ => Self::User,
    })
  }
}

impl std::fmt::Display for ChatRole {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", self.as_str())
  }
}

/// Classifies each LLM call for logging and analytics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmRequestType {
  /// Initial Bazi chart analysis
  BaziAnalysis,
  /// /date calendar → specific date selection
  CalendarDate,
  /// /date calendar → "Today" shortcut
  CalendarToday,
  /// /date calendar → "Tomorrow" shortcut
  CalendarTomorrow,
  /// /pick → date selection analysis
  PickSelection,
  /// Free-text follow-up chat via Telegram
  FollowUpChat,
  /// Bazi summary generation (internal, non-streaming)
  GenerateBaziSummary,
  /// Scheduled daily fortune cron job
  ScheduledDaily,
  /// API: /date-fortune WebSocket endpoint
  ApiDateFortune,
  /// API: /pick-date endpoint
  ApiPickDate,
  /// API: /chat endpoint
  ApiChat,
}

impl LlmRequestType {
  /// Returns the snake_case string for DB logging.
  /// Values match the existing strings in `llm_logs` for backward compatibility.
  pub fn as_str(&self) -> &'static str {
    match self {
      Self::BaziAnalysis => "bazi_analysis",
      Self::CalendarDate => "calendar_date",
      Self::CalendarToday => "calendar_today",
      Self::CalendarTomorrow => "calendar_tomorrow",
      Self::PickSelection => "pick_selection",
      Self::FollowUpChat => "follow_up_chat",
      Self::GenerateBaziSummary => "generate_bazi_summary",
      Self::ScheduledDaily => "scheduled_daily",
      Self::ApiDateFortune => "api_date_fortune",
      Self::ApiPickDate => "api_pick_date",
      Self::ApiChat => "api_chat",
    }
  }
}

/// Represents an LLM model configured in the database.
/// The database (llm_models table) is the single source of truth.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LlmModel {
  pub model_id: u8,
  pub model_name: String,
  pub is_enabled: bool,
}

impl LlmModel {
  pub fn new(model_id: u8, model_name: impl Into<String>, is_enabled: bool) -> Self {
    Self { model_id, model_name: model_name.into(), is_enabled }
  }

  pub fn as_str(&self) -> &str {
    &self.model_name
  }

  pub fn id(&self) -> u8 {
    self.model_id
  }
}

/// A stream is successful only after its producer explicitly acknowledges completion.
/// Dropped/failed producers cannot turn partial output into a successful reading.
pub struct LlmStream {
  receiver: tokio::sync::mpsc::Receiver<tokio_util::bytes::Bytes>,
  completion: tokio::sync::oneshot::Receiver<bool>,
}
impl LlmStream {
  pub fn new(receiver: tokio::sync::mpsc::Receiver<tokio_util::bytes::Bytes>, completion: tokio::sync::oneshot::Receiver<bool>) -> Self {
    Self { receiver, completion }
  }
  pub fn completed(receiver: tokio::sync::mpsc::Receiver<tokio_util::bytes::Bytes>) -> Self {
    let (sender, completion) = tokio::sync::oneshot::channel();
    let _ = sender.send(true);
    Self::new(receiver, completion)
  }
  pub async fn recv(&mut self) -> Option<tokio_util::bytes::Bytes> {
    self.receiver.recv().await
  }
  pub async fn finish(self) -> crate::models::AppResult<()> {
    if matches!(self.completion.await, Ok(true)) { Ok(()) } else { Err(crate::models::AppError::Message("Analysis stream interrupted".into())) }
  }
  /// Converts stream into an Axum-compatible byte stream yielding Result<Bytes, std::io::Error>
  pub fn into_byte_stream(self) -> impl futures::Stream<Item = Result<tokio_util::bytes::Bytes, std::io::Error>> {
    async_stream::try_stream! {
      let mut rx = self.receiver;
      let completion = self.completion;
      while let Some(chunk) = rx.recv().await {
        yield chunk;
      }
      if !matches!(completion.await, Ok(true)) {
        Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "Analysis stream interrupted"))?;
      }
    }
  }
}

/// User-selected or API-requested reasoning effort level for LLM generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningEffortLevel {
  #[default]
  None,
  Minimal,
  Low,
  Medium,
  High,
  Xhigh,
}

impl ReasoningEffortLevel {
  pub fn as_str(&self) -> &'static str {
    match self {
      Self::None => "none",
      Self::Minimal => "minimal",
      Self::Low => "low",
      Self::Medium => "medium",
      Self::High => "high",
      Self::Xhigh => "xhigh",
    }
  }

  pub fn parse(s: &str) -> Option<Self> {
    match s.trim().to_ascii_lowercase().as_str() {
      "none" => Some(Self::None),
      "minimal" => Some(Self::Minimal),
      "low" => Some(Self::Low),
      "medium" => Some(Self::Medium),
      "high" => Some(Self::High),
      "xhigh" => Some(Self::Xhigh),
      _ => None,
    }
  }

  /// Extracts an optional leading `#<level>` placeholder from the input text.
  /// If present, returns `(Some(level), remaining_text)`.
  /// If not present, returns `(None, original_text)`.
  /// Handles optional punctuation (e.g. `:`, `,`, `;`) and strips leading whitespace from remaining text.
  pub fn extract_leading_placeholder(text: &str) -> (Option<Self>, &str) {
    let trimmed = text.trim_start();
    if let Some(rest) = trimmed.strip_prefix('#') {
      let end_idx = rest.find(|c: char| c.is_whitespace() || matches!(c, ':' | ',' | ';')).unwrap_or(rest.len());
      let tag = &rest[..end_idx];
      if let Some(level) = Self::parse(tag) {
        let after_tag = &rest[end_idx..];
        let stripped = after_tag.trim_start_matches([':', ',', ';']).trim_start();
        return (Some(level), stripped);
      }
    }
    (None, text)
  }

  /// Maps to upstream provider reasoning effort.
  /// `None` is mapped to `None` so non-reasoning models (gpt-4o, gemini) do not reject the request.
  pub fn into_upstream(self) -> Option<async_openai::types::chat::ReasoningEffort> {
    match self {
      Self::None => None,
      Self::Minimal => Some(async_openai::types::chat::ReasoningEffort::Minimal),
      Self::Low => Some(async_openai::types::chat::ReasoningEffort::Low),
      Self::Medium => Some(async_openai::types::chat::ReasoningEffort::Medium),
      Self::High => Some(async_openai::types::chat::ReasoningEffort::High),
      Self::Xhigh => Some(async_openai::types::chat::ReasoningEffort::Xhigh),
    }
  }
}

#[cfg(test)]
mod stream_tests {
  use super::LlmStream;
  use tokio_util::bytes::Bytes;

  #[tokio::test]
  async fn partial_output_requires_explicit_success() {
    let (sender, receiver) = tokio::sync::mpsc::channel(1);
    let (completion, completed) = tokio::sync::oneshot::channel();
    sender.send(Bytes::from_static(b"partial")).await.unwrap();
    drop(sender);
    drop(completion);
    let mut stream = LlmStream::new(receiver, completed);
    assert_eq!(stream.recv().await.as_deref(), Some(&b"partial"[..]));
    assert!(stream.recv().await.is_none());
    assert!(stream.finish().await.is_err());
    let (sender, receiver) = tokio::sync::mpsc::channel(1);
    drop(sender);
    assert!(LlmStream::completed(receiver).finish().await.is_ok());
  }

  #[tokio::test]
  async fn into_byte_stream_reports_interrupted_error() {
    use futures::StreamExt;
    let (sender, receiver) = tokio::sync::mpsc::channel(1);
    let (completion, completed) = tokio::sync::oneshot::channel();
    sender.send(Bytes::from_static(b"hello")).await.unwrap();
    drop(sender);
    drop(completion);

    let stream = LlmStream::new(receiver, completed);
    let byte_stream = stream.into_byte_stream();
    tokio::pin!(byte_stream);
    assert_eq!(byte_stream.next().await.unwrap().unwrap().as_ref(), b"hello");
    assert!(byte_stream.next().await.unwrap().is_err());
  }

  #[test]
  fn test_reasoning_effort_level_mapping_and_parsing() {
    use super::ReasoningEffortLevel;

    assert_eq!(ReasoningEffortLevel::parse("none"), Some(ReasoningEffortLevel::None));
    assert_eq!(ReasoningEffortLevel::parse("HIGH"), Some(ReasoningEffortLevel::High));
    assert_eq!(ReasoningEffortLevel::parse("unknown"), None);

    assert_eq!(ReasoningEffortLevel::None.into_upstream(), None);
    assert_eq!(ReasoningEffortLevel::Low.into_upstream(), Some(async_openai::types::chat::ReasoningEffort::Low));
    assert_eq!(ReasoningEffortLevel::Medium.into_upstream(), Some(async_openai::types::chat::ReasoningEffort::Medium));
    assert_eq!(ReasoningEffortLevel::High.into_upstream(), Some(async_openai::types::chat::ReasoningEffort::High));

    // Test leading hashtag extraction
    assert_eq!(ReasoningEffortLevel::extract_leading_placeholder("#high 问财运"), (Some(ReasoningEffortLevel::High), "问财运"));
    assert_eq!(ReasoningEffortLevel::extract_leading_placeholder("#HIGH: 问财运"), (Some(ReasoningEffortLevel::High), "问财运"));
    assert_eq!(ReasoningEffortLevel::extract_leading_placeholder("#low, 问财运"), (Some(ReasoningEffortLevel::Low), "问财运"));
    assert_eq!(ReasoningEffortLevel::extract_leading_placeholder("#none 问财运"), (Some(ReasoningEffortLevel::None), "问财运"));
    assert_eq!(ReasoningEffortLevel::extract_leading_placeholder("#xhigh"), (Some(ReasoningEffortLevel::Xhigh), ""));
    assert_eq!(ReasoningEffortLevel::extract_leading_placeholder("问财运"), (None, "问财运"));
    assert_eq!(ReasoningEffortLevel::extract_leading_placeholder("#unknown 问财运"), (None, "#unknown 问财运"));
    assert_eq!(ReasoningEffortLevel::extract_leading_placeholder("   #medium 问财运"), (Some(ReasoningEffortLevel::Medium), "问财运"));
  }
}
