/// Represents a city and its geographical data.
#[derive(Debug, Clone)]
pub struct City {
    pub name: &'static str,
    pub longitude: f64,
}

/// A list of common cities for selection.
pub const COMMON_CITIES: &[City] = &[
    City {
        name: "吉隆坡 (Kuala Lumpur)",
        longitude: 101.68,
    },
    City {
        name: "文冬 (Bentong)",
        longitude: 101.91,
    },
    City {
        name: "马六甲 (Malacca)",
        longitude: 102.25,
    },
    City {
        name: "新加坡 (Singapore)",
        longitude: 103.85,
    },
    City {
        name: "北京 (Beijing)",
        longitude: 116.40,
    },
    City {
        name: "上海 (Shanghai)",
        longitude: 121.47,
    },
    City {
        name: "广州 (Guangzhou)",
        longitude: 113.26,
    },
    City {
        name: "深圳 (Shenzhen)",
        longitude: 114.05,
    },
    City {
        name: "香港 (Hong Kong)",
        longitude: 114.17,
    },
    City {
        name: "台北 (Taipei)",
        longitude: 121.56,
    },
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
        matches!(
            (self, target),
            (Self::Wood, Self::Earth) | (Self::Earth, Self::Water) | (Self::Water, Self::Fire) | (Self::Fire, Self::Metal) | (Self::Metal, Self::Wood)
        )
    }
    /// Generating cycle (生): Wood→Fire→Earth→Metal→Water→Wood
    pub fn generates(self, target: WuXing) -> bool {
        matches!(
            (self, target),
            (Self::Wood, Self::Fire) | (Self::Fire, Self::Earth) | (Self::Earth, Self::Metal) | (Self::Metal, Self::Water) | (Self::Water, Self::Wood)
        )
    }
}

// Calender
pub const MONTH_NAME: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
pub const DAY_HEADERS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/// LLM response: either a complete string or a streaming channel.
pub enum LlmResponse {
    Full(String),
    Stream(tokio::sync::mpsc::Receiver<String>),
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
