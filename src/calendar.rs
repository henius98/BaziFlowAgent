use chrono::{Datelike, NaiveDate};
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

/// Callback data prefix for calendar actions
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

/// Check if callback data is a calendar action
pub fn is_calendar_callback(data: &str) -> bool {
    data.starts_with(CAL_PREFIX)
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

/// Build an inline keyboard calendar for the given year and month
pub fn build_calendar(year: i32, month: u32) -> InlineKeyboardMarkup {
    let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();

    // Header row: << Year Month >>
    let month_names = [
        "", "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let header_text = format!("{} {}", month_names[month as usize], year);

    // Previous month calculation
    let (prev_year, prev_month) = if month == 1 {
        (year - 1, 12u32)
    } else {
        (year, month - 1)
    };
    // Next month calculation
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1u32)
    } else {
        (year, month + 1)
    };

    rows.push(vec![
        InlineKeyboardButton::callback(
            "◀️",
            CalendarAction::PrevMonth {
                year: prev_year,
                month: prev_month,
            }
            .encode(),
        ),
        InlineKeyboardButton::callback(header_text, CalendarAction::Ignore.encode()),
        InlineKeyboardButton::callback(
            "▶️",
            CalendarAction::NextMonth {
                year: next_year,
                month: next_month,
            }
            .encode(),
        ),
    ]);

    // Day-of-week header
    let day_headers = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];
    rows.push(
        day_headers
            .iter()
            .map(|&d| InlineKeyboardButton::callback(d, CalendarAction::Ignore.encode()))
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
        current_row.push(InlineKeyboardButton::callback(
            " ",
            CalendarAction::Ignore.encode(),
        ));
    }

    for day in 1..=total_days {
        let date = NaiveDate::from_ymd_opt(year, month, day).unwrap();
        current_row.push(InlineKeyboardButton::callback(
            day.to_string(),
            CalendarAction::SelectDate(date).encode(),
        ));

        if current_row.len() == 7 {
            rows.push(current_row.clone());
            current_row.clear();
        }
    }

    // Fill remaining cells in the last row
    if !current_row.is_empty() {
        while current_row.len() < 7 {
            current_row.push(InlineKeyboardButton::callback(
                " ",
                CalendarAction::Ignore.encode(),
            ));
        }
        rows.push(current_row);
    }

    // "Today" button row
    rows.push(vec![InlineKeyboardButton::callback(
        "📅 Today",
        CalendarAction::Today.encode(),
    )]);

    InlineKeyboardMarkup::new(rows)
}
