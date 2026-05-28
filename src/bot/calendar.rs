use crate::models::common::{DAY_HEADERS, MONTH_NAME};
use chrono::{Datelike, NaiveDate};
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

// ─────────────────────────────────────────────────────────────────────────────
// 1. Date Fortune calendar (/Date command)
// ─────────────────────────────────────────────────────────────────────────────

/// Callback data prefix for Date Fortune calendar actions
const CALENDER_PREFIX: &str = "cal";

/// Calendar action types encoded in callback data
#[derive(Debug, Clone)]
pub enum CalendarAction {
    /// User selected a specific date
    SelectDate(NaiveDate),
    /// Navigate to previous month
    PrevMonth { year: i32, month: u32 },
    /// Navigate to next month
    NextMonth { year: i32, month: u32 },
    /// Select today
    Today,
}

impl CalendarAction {
    /// Encode action into callback data string
    #[allow(dead_code)]
    pub fn encode(&self) -> String {
        match self {
            CalendarAction::SelectDate(date) => {
                format!("{}:sel:{}:{}:{}", CALENDER_PREFIX, date.year(), date.month(), date.day())
            }
            CalendarAction::PrevMonth { year, month } => {
                format!("{}:prev:{}:{}", CALENDER_PREFIX, year, month)
            }
            CalendarAction::NextMonth { year, month } => {
                format!("{}:next:{}:{}", CALENDER_PREFIX, year, month)
            }
            CalendarAction::Today => format!("{}:today", CALENDER_PREFIX),
        }
    }

    /// Decode callback data string into CalendarAction
    pub fn decode(data: &str) -> Option<CalendarAction> {
        let parts: Vec<&str> = data.split(':').collect();
        if parts.is_empty() || parts[0] != CALENDER_PREFIX {
            return None;
        }

        match parts.get(1).copied() {
            Some("sel") => {
                let year: i32 = parts.get(2)?.parse().ok()?;
                let month: u32 = parts.get(3)?.parse().ok()?;
                let day: u32 = parts.get(4)?.parse().ok()?;
                let date = NaiveDate::from_ymd_opt(year, month, day)?;
                Some(CalendarAction::SelectDate(date))
            }
            Some("prev") => {
                let year: i32 = parts.get(2)?.parse().ok()?;
                let month: u32 = parts.get(3)?.parse().ok()?;
                Some(CalendarAction::PrevMonth { year, month })
            }
            Some("next") => {
                let year: i32 = parts.get(2)?.parse().ok()?;
                let month: u32 = parts.get(3)?.parse().ok()?;
                Some(CalendarAction::NextMonth { year, month })
            }
            Some("today") => Some(CalendarAction::Today),
            _ => None,
        }
    }
}

/// Check if callback data is a Date Fortune calendar action
pub fn is_calendar_callback(data: &str) -> bool {
    data.starts_with(CALENDER_PREFIX) && !data.starts_with("bdcal")
}

/// Build an inline keyboard calendar for the given year and month (Date Fortune)
pub fn build_calendar(year: i32, month: u32) -> InlineKeyboardMarkup {
    build_calendar_inner(year, month, CALENDER_PREFIX)
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. Birthdate calendar  (/new command)
// ─────────────────────────────────────────────────────────────────────────────

/// Callback data prefix for birthdate picker calendar
const BDCALENDER_PREFIX: &str = "bdcal";

/// Birthdate calendar action types
#[derive(Debug, Clone)]
pub enum BirthdateCalAction {
    ViewYears { start_year: i32 },
    SelectYear(i32),
    SelectMonth { year: i32, month: u32 },
    SelectDate(NaiveDate),
    PrevMonth { year: i32, month: u32 },
    NextMonth { year: i32, month: u32 },
}

impl BirthdateCalAction {
    #[allow(dead_code)]
    pub fn encode(&self) -> String {
        match self {
            BirthdateCalAction::ViewYears { start_year } => {
                format!("{}:vy:{}", BDCALENDER_PREFIX, start_year)
            }
            BirthdateCalAction::SelectYear(year) => format!("{}:sy:{}", BDCALENDER_PREFIX, year),
            BirthdateCalAction::SelectMonth { year, month } => {
                format!("{}:sm:{}:{}", BDCALENDER_PREFIX, year, month)
            }
            BirthdateCalAction::SelectDate(date) => {
                format!("{}:sel:{}:{}:{}", BDCALENDER_PREFIX, date.year(), date.month(), date.day())
            }
            BirthdateCalAction::PrevMonth { year, month } => {
                format!("{}:prev:{}:{}", BDCALENDER_PREFIX, year, month)
            }
            BirthdateCalAction::NextMonth { year, month } => {
                format!("{}:next:{}:{}", BDCALENDER_PREFIX, year, month)
            }
        }
    }

    pub fn decode(data: &str) -> Option<BirthdateCalAction> {
        let parts: Vec<&str> = data.split(':').collect();
        if parts.is_empty() || parts[0] != BDCALENDER_PREFIX {
            return None;
        }

        match parts.get(1).copied() {
            Some("vy") => {
                let start_year: i32 = parts.get(2)?.parse().ok()?;
                Some(BirthdateCalAction::ViewYears { start_year })
            }
            Some("sy") => {
                let year: i32 = parts.get(2)?.parse().ok()?;
                Some(BirthdateCalAction::SelectYear(year))
            }
            Some("sm") => {
                let year: i32 = parts.get(2)?.parse().ok()?;
                let month: u32 = parts.get(3)?.parse().ok()?;
                Some(BirthdateCalAction::SelectMonth { year, month })
            }
            Some("sel") => {
                let year: i32 = parts.get(2)?.parse().ok()?;
                let month: u32 = parts.get(3)?.parse().ok()?;
                let day: u32 = parts.get(4)?.parse().ok()?;
                let date = NaiveDate::from_ymd_opt(year, month, day)?;
                Some(BirthdateCalAction::SelectDate(date))
            }
            Some("prev") => {
                let year: i32 = parts.get(2)?.parse().ok()?;
                let month: u32 = parts.get(3)?.parse().ok()?;
                Some(BirthdateCalAction::PrevMonth { year, month })
            }
            Some("next") => {
                let year: i32 = parts.get(2)?.parse().ok()?;
                let month: u32 = parts.get(3)?.parse().ok()?;
                Some(BirthdateCalAction::NextMonth { year, month })
            }
            _ => None,
        }
    }
}

/// Check if callback data is a birthdate calendar action
pub fn is_birthdate_cal_callback(data: &str) -> bool {
    data.starts_with(BDCALENDER_PREFIX)
}

/// Build an inline keyboard calendar for birthdate selection (/new command)
pub fn build_birthdate_calendar(year: i32, month: u32) -> InlineKeyboardMarkup {
    // Birthdate calendar uses bdcal prefix
    let mut markup = build_calendar_inner(year, month, BDCALENDER_PREFIX);

    // Add a Back to Month button
    let back_row = vec![InlineKeyboardButton::callback("◀️ Change Month", BirthdateCalAction::SelectYear(year).encode())];
    markup.inline_keyboard.push(back_row);
    markup
}

pub fn build_year_picker(start_year: i32) -> InlineKeyboardMarkup {
    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();

    // Grid of 12 years (3x4)
    for row_start in (0..12).step_by(3) {
        let mut row = Vec::new();
        for offset in 0..3 {
            let y = start_year + row_start + offset;
            row.push(InlineKeyboardButton::callback(y.to_string(), BirthdateCalAction::SelectYear(y).encode()));
        }
        rows.push(row);
    }

    // Nav row
    rows.push(vec![
        InlineKeyboardButton::callback("◀️ Prev 12", BirthdateCalAction::ViewYears { start_year: start_year - 12 }.encode()),
        InlineKeyboardButton::callback("Next 12 ▶️", BirthdateCalAction::ViewYears { start_year: start_year + 12 }.encode()),
    ]);

    InlineKeyboardMarkup::new(rows)
}

pub fn build_month_picker(year: i32) -> InlineKeyboardMarkup {
    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();

    // Grid of 12 months (3x4)
    for row_start in (0..12).step_by(3) {
        let mut row = Vec::new();
        for offset in 0..3 {
            let m_idx = row_start + offset;
            let m_num = (m_idx + 1) as u32;
            row.push(InlineKeyboardButton::callback(
                MONTH_NAME[m_idx as usize].to_string(),
                BirthdateCalAction::SelectMonth { year, month: m_num }.encode(),
            ));
        }
        rows.push(row);
    }

    // Back to year picker
    let start_year = year - (year % 12);
    rows.push(vec![InlineKeyboardButton::callback("◀️ Change Year", BirthdateCalAction::ViewYears { start_year }.encode())]);

    InlineKeyboardMarkup::new(rows)
}

// ─────────────────────────────────────────────────────────────────────────────
// 3 Gender Picker
// ─────────────────────────────────────────────────────────────────────────────

const BDGEN_PREFIX: &str = "bdgen";

#[derive(Debug, Clone)]
pub enum GenderAction {
    SelectMale,
    SelectFemale,
}

impl GenderAction {
    pub fn encode(&self) -> String {
        match self {
            GenderAction::SelectMale => format!("{}:m", BDGEN_PREFIX),
            GenderAction::SelectFemale => format!("{}:f", BDGEN_PREFIX),
        }
    }

    pub fn decode(data: &str) -> Option<GenderAction> {
        let parts: Vec<&str> = data.split(':').collect();
        if parts.is_empty() || parts[0] != BDGEN_PREFIX {
            return None;
        }

        match parts.get(1).copied() {
            Some("m") => Some(GenderAction::SelectMale),
            Some("f") => Some(GenderAction::SelectFemale),
            _ => None,
        }
    }
}

pub fn is_gender_picker_callback(data: &str) -> bool {
    data.starts_with(BDGEN_PREFIX)
}

pub fn build_gender_picker() -> InlineKeyboardMarkup {
    let rows = vec![vec![
        InlineKeyboardButton::callback("🧑 Male", GenderAction::SelectMale.encode()),
        InlineKeyboardButton::callback("👩 Female", GenderAction::SelectFemale.encode()),
    ]];
    InlineKeyboardMarkup::new(rows)
}

// ─────────────────────────────────────────────────────────────────────────────
// 4 Location Picker
// ─────────────────────────────────────────────────────────────────────────────

const BDLOC_PREFIX: &str = "bdloc";

#[derive(Debug, Clone)]
pub enum LocationAction {
    SelectCity(String),
    Skip,
}

impl LocationAction {
    pub fn encode(&self) -> String {
        match self {
            LocationAction::SelectCity(name) => format!("{}:sc:{}", BDLOC_PREFIX, name),
            LocationAction::Skip => format!("{}:skip", BDLOC_PREFIX),
        }
    }

    pub fn decode(data: &str) -> Option<LocationAction> {
        let parts: Vec<&str> = data.split(':').collect();
        if parts.is_empty() || parts[0] != BDLOC_PREFIX {
            return None;
        }

        match parts.get(1).copied() {
            Some("sc") => {
                let name = parts.get(2)?.to_string();
                Some(LocationAction::SelectCity(name))
            }
            Some("skip") => Some(LocationAction::Skip),
            _ => None,
        }
    }
}

pub fn is_location_picker_callback(data: &str) -> bool {
    data.starts_with(BDLOC_PREFIX)
}

pub fn build_location_picker() -> InlineKeyboardMarkup {
    use crate::models::common::COMMON_CITIES;
    let cities = COMMON_CITIES;
    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();

    // 2 cities per row
    for chunk in cities.chunks(2) {
        let mut row = Vec::new();
        for city in chunk {
            row.push(InlineKeyboardButton::callback(city.name, LocationAction::SelectCity(city.name.to_string()).encode()));
        }
        rows.push(row);
    }

    // Skip/Other button
    rows.push(vec![InlineKeyboardButton::callback("⏩ Skip / Default (120°E)", LocationAction::Skip.encode())]);

    InlineKeyboardMarkup::new(rows)
}

// ─────────────────────────────────────────────────────────────────────────────
// 5 Time Picker
// ─────────────────────────────────────────────────────────────────────────────

const BDTIME_PREFIX: &str = "bdtime";

#[derive(Debug, Clone)]
pub enum TimeAction {
    SelectHour(u32),
    SelectMinute { hour: u32, minute: u32 },
    BackToHour,
}

impl TimeAction {
    pub fn encode(&self) -> String {
        match self {
            TimeAction::SelectHour(h) => format!("{}:sh:{}", BDTIME_PREFIX, h),
            TimeAction::SelectMinute { hour, minute } => {
                format!("{}:sm:{}:{}", BDTIME_PREFIX, hour, minute)
            }
            TimeAction::BackToHour => format!("{}:back_h", BDTIME_PREFIX),
        }
    }

    pub fn decode(data: &str) -> Option<TimeAction> {
        let parts: Vec<&str> = data.split(':').collect();
        if parts.is_empty() || parts[0] != BDTIME_PREFIX {
            return None;
        }

        match parts.get(1).copied() {
            Some("sh") => {
                let h: u32 = parts.get(2)?.parse().ok()?;
                Some(TimeAction::SelectHour(h))
            }
            Some("sm") => {
                let h: u32 = parts.get(2)?.parse().ok()?;
                let m: u32 = parts.get(3)?.parse().ok()?;
                Some(TimeAction::SelectMinute { hour: h, minute: m })
            }
            Some("back_h") => Some(TimeAction::BackToHour),
            _ => None,
        }
    }
}

pub fn is_time_picker_callback(data: &str) -> bool {
    data.starts_with(BDTIME_PREFIX)
}

pub fn build_hour_picker() -> InlineKeyboardMarkup {
    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();

    // 24 hours in 4x6 grid
    for row_idx in 0..6 {
        let mut row = Vec::new();
        for col_idx in 0..4 {
            let h = row_idx * 4 + col_idx;
            row.push(InlineKeyboardButton::callback(format!("{:02}:00", h), TimeAction::SelectHour(h).encode()));
        }
        rows.push(row);
    }

    InlineKeyboardMarkup::new(rows)
}

pub fn build_minute_picker(hour: u32) -> InlineKeyboardMarkup {
    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();

    // 60 minutes in an 8-column grid (Telegram's standard row limit)
    for m in 0..60 {
        if m % 8 == 0 {
            rows.push(Vec::new());
        }
        if let Some(row) = rows.last_mut() {
            row.push(InlineKeyboardButton::callback(format!("{:02}", m), TimeAction::SelectMinute { hour, minute: m }.encode()));
        }
    }

    // Add a back button
    rows.push(vec![InlineKeyboardButton::callback("◀️ Back to Hour", TimeAction::BackToHour.encode())]);

    InlineKeyboardMarkup::new(rows)
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. Shared calendar builder
// ─────────────────────────────────────────────────────────────────────────────

/// Internal calendar builder shared between Date Fortune and birthdate pickers
fn build_calendar_inner(year: i32, month: u32, prefix: &str) -> InlineKeyboardMarkup {
    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();

    // Header row: ◀️ Month Year ▶️
    let header_text = format!("{} {}", MONTH_NAME[(month - 1) as usize], year);

    let (prev_year, prev_month) = if month == 1 { (year - 1, 12u32) } else { (year, month - 1) };
    let (next_year, next_month) = if month == 12 { (year + 1, 1u32) } else { (year, month + 1) };

    let ignore_cb = format!("{}:ignore", prefix);
    let prev_cb = format!("{}:prev:{}:{}", prefix, prev_year, prev_month);
    let next_cb = format!("{}:next:{}:{}", prefix, next_year, next_month);

    rows.push(vec![
        InlineKeyboardButton::callback("◀️", prev_cb),
        InlineKeyboardButton::callback(header_text, ignore_cb.clone()),
        InlineKeyboardButton::callback("▶️", next_cb),
    ]);

    // Day-of-week header
    rows.push(DAY_HEADERS.iter().map(|&d| InlineKeyboardButton::callback(d, ignore_cb.clone())).collect());

    // Calendar grid
    let first_day = match NaiveDate::from_ymd_opt(year, month, 1) {
        Some(d) => d,
        None => return InlineKeyboardMarkup::new(rows), // Invalid date, return partial
    };
    // Monday = 0, Sunday = 6
    let start_weekday = first_day.weekday().num_days_from_monday() as usize;
    let total_days = days_in_month(year, month);

    let mut current_row: Vec<InlineKeyboardButton> = Vec::new();

    // Fill empty cells before the first day
    for _ in 0..start_weekday {
        current_row.push(InlineKeyboardButton::callback(" ", ignore_cb.clone()));
    }

    for day in 1..=total_days {
        let sel_cb = format!("{}:sel:{}:{}:{}", prefix, year, month, day);
        current_row.push(InlineKeyboardButton::callback(day.to_string(), sel_cb));

        if current_row.len() == 7 {
            rows.push(current_row.clone());
            current_row.clear();
        }
    }

    // Fill remaining cells in the last row
    if !current_row.is_empty() {
        while current_row.len() < 7 {
            current_row.push(InlineKeyboardButton::callback(" ", ignore_cb.clone()));
        }
        rows.push(current_row);
    }

    // Optional "Today" button
    if prefix == CALENDER_PREFIX {
        rows.push(vec![InlineKeyboardButton::callback("📅 Today", format!("{}:today", prefix))]);
    }

    InlineKeyboardMarkup::new(rows)
}

/// Get the number of days in a given month
fn days_in_month(year: i32, month: u32) -> u32 {
    // Navigate to the first day of the next month, then subtract one day
    let next_month_first = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    };
    next_month_first.and_then(|d| d.pred_opt()).map(|d| d.day()).unwrap_or(30) // Safe fallback for edge cases
}
