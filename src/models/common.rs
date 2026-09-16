/// Represents a city and its geographical data.
#[derive(Debug, Clone)]
pub struct City {
  pub name: &'static str,
  pub longitude: f64,
}

/// A list of common cities for selection.
pub const COMMON_CITIES: &[City] = &[
  City { name: "吉隆坡 (Kuala Lumpur)", longitude: 101.68 },
  City { name: "文冬 (Bentong)", longitude: 101.91 },
  City { name: "马六甲 (Malacca)", longitude: 102.25 },
  City { name: "新加坡 (Singapore)", longitude: 103.85 },
  City { name: "北京 (Beijing)", longitude: 116.40 },
  City { name: "上海 (Shanghai)", longitude: 121.47 },
  City { name: "广州 (Guangzhou)", longitude: 113.26 },
  City { name: "深圳 (Shenzhen)", longitude: 114.05 },
  City { name: "香港 (Hong Kong)", longitude: 114.17 },
  City { name: "台北 (Taipei)", longitude: 121.56 },
];

// Bazi records
pub const STEMS: [&str; 10] = ["甲", "乙", "丙", "丁", "戊", "己", "庚", "辛", "壬", "癸"];
pub const BRANCHES: [&str; 12] = ["子", "丑", "寅", "卯", "辰", "巳", "午", "未", "申", "酉", "戌", "亥"];
pub const STATES: [&str; 12] = ["长生", "沐浴", "冠带", "临官", "帝旺", "衰", "病", "死", "墓", "绝", "胎", "养"];

/// Five Element (五行) type for the overcoming/destroying (克) cycle
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum WuXing {
  Wood,  // 木
  Fire,  // 火
  Earth, // 土
  Metal, // 金
  Water, // 水
}
impl WuXing {
  /// Overcoming/Destroying cycle (克): Wood→Earth→Water→Fire→Metal→Wood
  pub fn destroys(self, target: WuXing) -> bool {
    matches!((self, target), (Self::Wood, Self::Earth) | (Self::Earth, Self::Water) | (Self::Water, Self::Fire) | (Self::Fire, Self::Metal) | (Self::Metal, Self::Wood))
  }
  /// Generating cycle (生): Wood→Fire→Earth→Metal→Water→Wood
  pub fn generates(self, target: WuXing) -> bool {
    matches!((self, target), (Self::Wood, Self::Fire) | (Self::Fire, Self::Earth) | (Self::Earth, Self::Metal) | (Self::Metal, Self::Water) | (Self::Water, Self::Wood))
  }
}

// Calendar
pub const MONTH_NAME: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
pub const DAY_HEADERS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/// LLM response: either a complete string or a streaming channel.
pub enum LlmResponse {
  Full(String),
  Stream(LlmStream),
}

/// Classifies each LLM call for logging and analytics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmRequestType {
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

macro_rules! define_llm_models {
    (
        $(
            $variant:ident = $val:expr => $str_name:expr
        ),* $(,)?
    ) => {
        /// Represents the LLM model used by a user.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #[repr(u8)]
        pub enum LlmModel {
            $( $variant = $val, )*
        }

        impl LlmModel {
            pub const ALL: &'static [Self] = &[
                $( Self::$variant, )*
            ];

            pub fn from_u8(val: u8) -> Option<Self> {
                match val {
                    $( $val => Some(Self::$variant), )*
                    _ => None,
                }
            }

            pub fn as_str(&self) -> &'static str {
                match self {
                    $( Self::$variant => $str_name, )*
                }
            }
        }
    };
}

define_llm_models! {
    Claude48Opus = 0 => "anthropic/claude-opus-4.8",
    Gpt55Pro = 1 => "openai/gpt-5.5-pro",
    Gemini31Pro = 2 => "google/gemini-3.1-pro-preview",
}

/// A stream is successful only after its producer explicitly acknowledges completion.
/// Dropped/failed producers cannot turn partial output into a successful reading.
pub struct LlmStream {
  receiver: tokio::sync::mpsc::Receiver<String>,
  completion: tokio::sync::oneshot::Receiver<bool>,
}
impl LlmStream {
  pub fn new(receiver: tokio::sync::mpsc::Receiver<String>, completion: tokio::sync::oneshot::Receiver<bool>) -> Self {
    Self { receiver, completion }
  }
  pub fn completed(receiver: tokio::sync::mpsc::Receiver<String>) -> Self {
    let (sender, completion) = tokio::sync::oneshot::channel();
    let _ = sender.send(true);
    Self::new(receiver, completion)
  }
  pub async fn recv(&mut self) -> Option<String> {
    self.receiver.recv().await
  }
  pub async fn finish(self) -> crate::models::AppResult<()> {
    if matches!(self.completion.await, Ok(true)) { Ok(()) } else { Err(crate::models::AppError::Message("Analysis stream interrupted".into())) }
  }
}

#[cfg(test)]
mod stream_tests {
  use super::LlmStream;
  #[tokio::test]
  async fn partial_output_requires_explicit_success() {
    let (sender, receiver) = tokio::sync::mpsc::channel(1);
    let (completion, completed) = tokio::sync::oneshot::channel();
    sender.send("partial".into()).await.unwrap();
    drop(sender);
    drop(completion);
    let mut stream = LlmStream::new(receiver, completed);
    assert_eq!(stream.recv().await.as_deref(), Some("partial"));
    assert!(stream.recv().await.is_none());
    assert!(stream.finish().await.is_err());
    let (sender, receiver) = tokio::sync::mpsc::channel(1);
    drop(sender);
    assert!(LlmStream::completed(receiver).finish().await.is_ok());
  }
}
