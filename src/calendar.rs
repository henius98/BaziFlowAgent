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
    ViewYears { start_year: i32 },
    SelectYear(i32),
    SelectMonth { year: i32, month: u32 },
    SelectDate(NaiveDate),
    PrevMonth { year: i32, month: u32 },
    NextMonth { year: i32, month: u32 },
    Ignore,
}

impl BirthdateCalAction {
    #[allow(dead_code)]
    pub fn encode(&self) -> String {
        match self {
            BirthdateCalAction::ViewYears { start_year } => format!("{}:vy:{}", BDCAL_PREFIX, start_year),
            BirthdateCalAction::SelectYear(year) => format!("{}:sy:{}", BDCAL_PREFIX, year),
            BirthdateCalAction::SelectMonth { year, month } => format!("{}:sm:{}:{}", BDCAL_PREFIX, year, month),
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
    let mut markup = build_calendar_inner(year, month, BDCAL_PREFIX, false);
    
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
            row.push(InlineKeyboardButton::callback(
                y.to_string(), 
                BirthdateCalAction::SelectYear(y).encode()
            ));
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
    let month_names = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

    // Grid of 12 months (3x4)
    for row_start in (0..12).step_by(3) {
        let mut row = Vec::new();
        for offset in 0..3 {
            let m_idx = row_start + offset;
            let m_num = (m_idx + 1) as u32;
            row.push(InlineKeyboardButton::callback(
                month_names[m_idx as usize].to_string(),
                BirthdateCalAction::SelectMonth { year, month: m_num }.encode()
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
// 2.5 Gender Picker
// ─────────────────────────────────────────────────────────────────────────────

const BDGEN_PREFIX: &str = "bdgen";

#[derive(Debug, Clone)]
pub enum GenderAction {
    SelectMale,
    SelectFemale,
    Ignore,
}

impl GenderAction {
    pub fn encode(&self) -> String {
        match self {
            GenderAction::SelectMale => format!("{}:m", BDGEN_PREFIX),
            GenderAction::SelectFemale => format!("{}:f", BDGEN_PREFIX),
            GenderAction::Ignore => format!("{}:ignore", BDGEN_PREFIX),
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
            Some("ignore") => Some(GenderAction::Ignore),
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

