//! Shared utility functions used across bot handlers, scheduler, and other modules.

use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone};

/// Split a long message into chunks that fit within Telegram's message size limit.
pub fn split_message(text: &str, limit: usize) -> Vec<String> {
  if text.is_empty() {
    return vec![String::new()];
  }
  // Count UTF-16 units conservatively, and preserve every byte including newlines.
  let limit = limit.max(2);
  let mut result = Vec::new();
  let mut start = 0;
  let mut units = 0;
  for (offset, character) in text.char_indices() {
    let width = character.len_utf16();
    if units + width > limit {
      result.push(text[start..offset].to_owned());
      start = offset;
      units = 0;
    }
    units += width;
  }
  result.push(text[start..].to_owned());
  result
}

use crate::models::error::AppResult;
use crate::services::paipan::StructuredBazi;

/// Parse the raw JSON string of a user's Bazi chart.
pub fn parse_user_bazi(raw: Option<&str>) -> AppResult<StructuredBazi> {
  StructuredBazi::from_json(raw)
}

/// Format a Unix epoch timestamp (seconds) into standard `%Y-%m-%d %H:%M:%S` in UTC+8 (SGT/CST).
pub fn format_timestamp_utc8(timestamp: i64) -> Option<String> {
  let offset = FixedOffset::east_opt(8 * 3600)?;
  DateTime::from_timestamp(timestamp, 0).map(|dt| dt.with_timezone(&offset).format("%Y-%m-%d %H:%M:%S").to_string())
}

/// Convert a standard `%Y-%m-%d %H:%M:%S` datetime string in UTC+8 back to a Unix epoch timestamp (seconds).
pub fn parse_timestamp_utc8(datetime_str: &str) -> Option<i64> {
  let naive = NaiveDateTime::parse_from_str(datetime_str, "%Y-%m-%d %H:%M:%S").ok()?;
  let offset = FixedOffset::east_opt(8 * 3600)?;
  offset.from_local_datetime(&naive).single().map(|dt| dt.timestamp())
}

#[cfg(test)]
mod split_tests {
  #[test]
  fn long_lines_and_unicode_are_bounded_and_lossless() {
    for text in ["a".repeat(20000), "命😀\n".repeat(5000), "\nhello\n".to_owned()] {
      let chunks = super::split_message(&text, 4000);
      assert_eq!(chunks.concat(), text);
      assert!(chunks.iter().all(|s| !s.is_empty() && s.encode_utf16().count() <= 4000));
    }
  }
}

#[cfg(test)]
mod timestamp_tests {
  use super::*;

  #[test]
  fn format_and_parse_timestamp_utc8_round_trip() {
    let ts = 1791446840; // 2026-10-08 16:07:20 SGT
    let formatted = format_timestamp_utc8(ts).expect("formatted timestamp");
    assert_eq!(formatted, "2026-10-08 16:07:20");

    let parsed = parse_timestamp_utc8(&formatted).expect("parsed timestamp");
    assert_eq!(parsed, ts);

    // Epoch 0 in UTC is 08:00:00 in UTC+8
    assert_eq!(format_timestamp_utc8(0), Some("1970-01-01 08:00:00".to_string()));
    assert_eq!(parse_timestamp_utc8("1970-01-01 08:00:00"), Some(0));
  }
}
