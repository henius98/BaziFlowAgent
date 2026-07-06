use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────
// Request types
// ─────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateProfileRequest {
    pub gender: u8,
    pub birth_date: String,
    pub birth_hour: u8,
    pub birth_minute: u8,
    pub location: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DateFortuneRequest {
    pub date: String,
}

#[derive(Debug, Deserialize)]
pub struct PickDateRequest {
    pub start_date: String,
    pub end_date: String,
    pub activity: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateModelRequest {
    pub model: u8,
}

#[derive(Debug, Deserialize)]
pub struct UpdateScheduleRequest {
    pub time: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub message: String,
}

// ─────────────────────────────────────────────
// Response types
// ─────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub status: String,
    #[serde(flatten)]
    pub data: T,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self {
            status: "ok".to_string(),
            data,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ApiError {
    pub status: String,
    pub message: String,
}

impl ApiError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            status: "error".to_string(),
            message: message.into(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ProfileData {
    pub profile: ProfileDetail,
}

#[derive(Debug, Serialize)]
pub struct ProfileDetail {
    pub gender: Option<String>,
    pub solar_date: Option<String>,
    pub lunar_date: Option<String>,
    pub pillars: Option<serde_json::Value>,
    pub chart_url: Option<String>,
    pub bazi_analysis: Option<String>,
    pub bazi_summary: Option<String>,
    pub llm_model: Option<String>,
    pub schedule: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreatedProfileData {
    pub chart_url: String,
    pub bazi_analysis: String,
    pub bazi_summary: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FortuneData {
    pub almanac: String,
    pub analysis: String,
}

#[derive(Debug, Serialize)]
pub struct PickData {
    pub analysis: String,
}

#[derive(Debug, Serialize)]
pub struct ModelData {
    pub model: String,
}

#[derive(Debug, Serialize)]
pub struct ScheduleData {
    pub schedule: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ChatData {
    pub reply: String,
}
