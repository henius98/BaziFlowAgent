use chrono::{Datelike, NaiveDate};
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

// ─────────────────────────────────────────────────────────────────────────────
// 1. Bazi analysis calendar  (existing, unchanged)
// ─────────────────────────────────────────────────────────────────────────────

/// Callback data prefix for Bazi analysis calendar actions
const CAL_PREFIX: &str = "cal";

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
    /// Ignore (empty cells, header)
    Ignore,
}

impl CalendarAction {
    /// Encode action into callback data string
    #[allow(dead_code)]
    pub fn encode(&self) -> String {
        match self {
            CalendarAction::SelectDate(date) => {
                format!("{}:sel:{}:{}:{}", CAL_PREFIX, date.year(), date.month(), date.day())
            }
            CalendarAction::PrevMonth { year, month } => {
                format!("{}:prev:{}:{}", CAL_PREFIX, year, month)
            }
            CalendarAction::NextMonth { year, month } => {
                format!("{}:next:{}:{}", CAL_PREFIX, year, month)
            }
            CalendarAction::Today => format!("{}:today", CAL_PREFIX),
            CalendarAction::Ignore => format!("{}:ignore", CAL_PREFIX),
        }
    }

    /// Decode callback data string into CalendarAction
    pub fn decode(data: &str) -> Option<CalendarAction> {
        let parts: Vec<&str> = data.split(':').collect();
        if parts.is_empty() || parts[0] != CAL_PREFIX {
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
            Some("ignore") => Some(CalendarAction::Ignore),
            _ => None,
        }
    }
}

/// Check if callback data is a Bazi analysis calendar action
pub fn is_calendar_callback(data: &str) -> bool {
    data.starts_with(CAL_PREFIX) && !data.starts_with("bdcal")
}

/// Get the number of days in a given month
fn days_in_month(year: i32, month: u32) -> u32 {
    // Navigate to the first day of the next month, then subtract one day
    if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    }
    .unwrap()
    .pred_opt()
    .unwrap()
    .day()
}

/// Build an inline keyboard calendar for the given year and month (Bazi analysis)
pub fn build_calendar(year: i32, month: u32) -> InlineKeyboardMarkup {
    build_calendar_inner(year, month, CAL_PREFIX, true)
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. Birthdate calendar  (/new command)
// ─────────────────────────────────────────────────────────────────────────────

/// Callback data prefix for birthdate picker calendar
const BDCAL_PREFIX: &str = "bdcal";

/// Birthdate calendar action types
#[derive(Debug, Clone)]
pub enum BirthdateCalAction {
    SelectDate(NaiveDate),
    PrevMonth { year: i32, month: u32 },
    NextMonth { year: i32, month: u32 },
    Ignore,
}

impl BirthdateCalAction {
    #[allow(dead_code)]
    pub fn encode(&self) -> String {
        match self {
            BirthdateCalAction::SelectDate(date) => {
                format!("{}:sel:{}:{}:{}", BDCAL_PREFIX, date.year(), date.month(), date.day())
            }
            BirthdateCalAction::PrevMonth { year, month } => {
                format!("{}:prev:{}:{}", BDCAL_PREFIX, year, month)
            }
            BirthdateCalAction::NextMonth { year, month } => {
                format!("{}:next:{}:{}", BDCAL_PREFIX, year, month)
            }
            BirthdateCalAction::Ignore => format!("{}:ignore", BDCAL_PREFIX),
        }
    }

    pub fn decode(data: &str) -> Option<BirthdateCalAction> {
        let parts: Vec<&str> = data.split(':').collect();
        if parts.is_empty() || parts[0] != BDCAL_PREFIX {
            return None;
        }

        match parts.get(1).copied() {
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
            Some("ignore") => Some(BirthdateCalAction::Ignore),
            _ => None,
        }
    }
}

/// Check if callback data is a birthdate calendar action
pub fn is_birthdate_cal_callback(data: &str) -> bool {
    data.starts_with(BDCAL_PREFIX)
}

/// Build an inline keyboard calendar for birthdate selection (/new command)
pub fn build_birthdate_calendar(year: i32, month: u32) -> InlineKeyboardMarkup {
    // Birthdate calendar has no "Today" button and uses bdcal prefix
    build_calendar_inner(year, month, BDCAL_PREFIX, false)
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. Shared calendar builder
// ─────────────────────────────────────────────────────────────────────────────

/// Internal calendar builder shared between Bazi analysis and birthdate pickers
fn build_calendar_inner(
    year: i32,
    month: u32,
    prefix: &str,
    show_today: bool,
) -> InlineKeyboardMarkup {
    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();

    // Header row: ◀️ Month Year ▶️
    let month_names = [
        "", "Jan", "Feb", "Mar", "Apr", "May", "Jun",
        "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let header_text = format!("{} {}", month_names[month as usize], year);

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
    let day_headers = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];
    rows.push(
        day_headers
            .iter()
            .map(|&d| InlineKeyboardButton::callback(d, ignore_cb.clone()))
            .collect(),
    );

    // Calendar grid
    let first_day = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    // Monday = 0, Sunday = 6
    let start_weekday = first_day.weekday().num_days_from_monday() as usize;
    let total_days = days_in_month(year, month);

    let mut current_row: Vec<InlineKeyboardButton> = Vec::new();

    // Fill empty cells before the first day
    for _ in 0..start_weekday {
        current_row.push(InlineKeyboardButton::callback(" ", ignore_cb.clone()));
    }

    for day in 1..=total_days {
        let _date = NaiveDate::from_ymd_opt(year, month, day).unwrap();
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
    if show_today {
        rows.push(vec![InlineKeyboardButton::callback(
            "📅 Today",
            format!("{}:today", prefix),
        )]);
    }

    InlineKeyboardMarkup::new(rows)
}

// ─────────────────────────────────────────────────────────────────────────────
// 4. Birth-time picker  (hour → minute two-step inline keyboard)
// ─────────────────────────────────────────────────────────────────────────────

/// Callback data prefix for time picker actions
const BDTIME_PREFIX: &str = "bdtime";

/// Time picker action types
/// Encoded as compact strings to stay under Telegram's 64-byte callback limit.
#[derive(Debug, Clone)]
pub enum TimePickerAction {
    /// User selected an hour; show minute picker next
    SelectHour { date: String, hour: u32 },
    /// User selected a minute; ready to save
    SelectMinute { date: String, hour: u32, minute: u32 },
    /// Go back to the hour picker
    BackToHours { date: String },
    /// Ignore (header / decorative cells)
    Ignore,
}

impl TimePickerAction {
    /// Encode:
    ///   SelectHour   → `bdtime:h:{HH}:{YYYY-MM-DD}`      (≤ 24 chars)
    ///   SelectMinute → `bdtime:m:{HH}:{MM}:{YYYY-MM-DD}` (≤ 27 chars)
    ///   Ignore       → `bdtime:ignore`
    pub fn encode(&self) -> String {
        match self {
            TimePickerAction::SelectHour { date, hour } => {
                format!("{}:h:{:02}:{}", BDTIME_PREFIX, hour, date)
            }
            TimePickerAction::SelectMinute { date, hour, minute } => {
                format!("{}:m:{:02}:{:02}:{}", BDTIME_PREFIX, hour, minute, date)
            }
            TimePickerAction::BackToHours { date } => {
                format!("{}:back:{}", BDTIME_PREFIX, date)
            }
            TimePickerAction::Ignore => format!("{}:ignore", BDTIME_PREFIX),
        }
    }

    pub fn decode(data: &str) -> Option<TimePickerAction> {
        let parts: Vec<&str> = data.splitn(5, ':').collect();
        if parts.is_empty() || parts[0] != BDTIME_PREFIX {
            return None;
        }

        match parts.get(1).copied() {
            Some("h") => {
                // bdtime:h:{HH}:{YYYY-MM-DD}  → after splitn(5): ["bdtime","h","08","2026-01-15"]
                let hour: u32 = parts.get(2)?.parse().ok()?;
                let date = parts.get(3)?.to_string();
                Some(TimePickerAction::SelectHour { date, hour })
            }
            Some("m") => {
                // bdtime:m:{HH}:{MM}:{YYYY-MM-DD} → splitn(5): ["bdtime","m","08","30","2026-01-15"]
                let hour: u32 = parts.get(2)?.parse().ok()?;
                let minute: u32 = parts.get(3)?.parse().ok()?;
                let date = parts.get(4)?.to_string();
                Some(TimePickerAction::SelectMinute { date, hour, minute })
            }
            Some("back") => {
                // bdtime:back:{YYYY-MM-DD} → splitn(3): ["bdtime","back","2026-01-15"]
                let parts3: Vec<&str> = data.splitn(3, ':').collect();
                let date = parts3.get(2)?.to_string();
                Some(TimePickerAction::BackToHours { date })
            }
            Some("ignore") => Some(TimePickerAction::Ignore),
            _ => None,
        }
    }
}

/// Check if callback data is a time picker action
pub fn is_time_picker_callback(data: &str) -> bool {
    data.starts_with(BDTIME_PREFIX)
}

/// Build an hour picker inline keyboard after user selects their birthdate.
/// Shows 24 hours in rows of 6.
pub fn build_hour_picker(date: &str) -> InlineKeyboardMarkup {
    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();

    // Header
    let ignore_cb = TimePickerAction::Ignore.encode();
    rows.push(vec![InlineKeyboardButton::callback(
        format!("📅 {} — Select birth hour (0–23):", date),
        ignore_cb.clone(),
    )]);

    // Hours 00–23 in rows of 6
    for row_start in (0u32..24).step_by(6) {
        let row: Vec<InlineKeyboardButton> = (row_start..row_start + 6)
            .map(|h| {
                InlineKeyboardButton::callback(
                    format!("{:02}", h),
                    TimePickerAction::SelectHour { date: date.to_string(), hour: h }.encode(),
                )
            })
            .collect();
        rows.push(row);
    }

    InlineKeyboardMarkup::new(rows)
}

/// Build a minute picker inline keyboard after user selects an hour.
/// Shows common minute values: :00  :15  :30  :45
pub fn build_minute_picker(date: &str, hour: u32) -> InlineKeyboardMarkup {
    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();

    let ignore_cb = TimePickerAction::Ignore.encode();
    rows.push(vec![InlineKeyboardButton::callback(
        format!("🕐 {:02}:__ — Select birth minute:", hour),
        ignore_cb.clone(),
    )]);

    // Minute options
    let minutes = [0u32, 15, 30, 45];
    let minute_row: Vec<InlineKeyboardButton> = minutes
        .iter()
        .map(|&m| {
            InlineKeyboardButton::callback(
                format!(":{:02}", m),
                TimePickerAction::SelectMinute {
                    date: date.to_string(),
                    hour,
                    minute: m,
                }
                .encode(),
            )
        })
        .collect();
    rows.push(minute_row);

    // Back to hour picker
    rows.push(vec![InlineKeyboardButton::callback(
        "◀️ Change hour",
        TimePickerAction::BackToHours { date: date.to_string() }.encode(),
    )]);

    InlineKeyboardMarkup::new(rows)
}
