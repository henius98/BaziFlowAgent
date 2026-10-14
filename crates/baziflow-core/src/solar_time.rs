use super::common::COMMON_CITIES;
use chrono::{Datelike, Duration, NaiveDateTime};

/// Default reference meridian for China / Singapore standard time (UTC+8).
pub const BEIJING_MERIDIAN: f64 = 120.0;

/// Converts Standard Time to True Solar Time (真太阳时) for an optional location.
///
/// If location is None or not found in known cities, returns origin_time unmodified.
/// Uses the Beijing standard meridian (120.0°E) by default.
pub fn to_true_solar_time(origin_time: NaiveDateTime, location: Option<&str>) -> NaiveDateTime {
  match location {
    Some(city) => calculate_true_solar_time(origin_time, city, BEIJING_MERIDIAN),
    None => origin_time,
  }
}

/// Converts Standard Time (usually Beijing Time / UTC+8) to True Solar Time (真太阳时).
pub fn calculate_true_solar_time(origin_time: NaiveDateTime, city_name: &str, standard_meridian: f64) -> NaiveDateTime {
  let longitude = COMMON_CITIES.iter().find(|c| c.name.contains(city_name) || city_name.contains(c.name)).map(|c| c.longitude);
  if let Some(lon) = longitude {
    // 1. Longitude Adjustment: 4 minutes per degree
    let longitude_diff = lon - standard_meridian;
    let longitude_adjustment_mins = longitude_diff * 4.0;

    // 2. Equation of Time Adjustment
    let day_of_year = origin_time.ordinal();
    let eot_adjustment_mins = get_equation_of_time(day_of_year);

    // Total adjustment in seconds
    let total_adjustment_secs = ((longitude_adjustment_mins + eot_adjustment_mins) * 60.0).round() as i64;

    origin_time + Duration::seconds(total_adjustment_secs)
  } else {
    origin_time
  }
}

/// Calculates the Equation of Time (EoT) in minutes for a given day of the year.
/// This uses a standard approximation formula.
pub fn get_equation_of_time(day_of_year: u32) -> f64 {
  let b = 2.0 * std::f64::consts::PI * (day_of_year as f64 - 81.0) / 365.0;
  9.87 * (2.0 * b).sin() - 7.53 * b.cos() - 1.5 * b.sin()
}

#[cfg(test)]
mod tests {
  use super::*;
  use chrono::{NaiveDate, Timelike};

  #[test]
  fn test_malacca_conversion() {
    let dt = NaiveDate::from_ymd_opt(1998, 10, 8).and_then(|d| d.and_hms_opt(7, 21, 0)).expect("valid test date");
    let sun_time = calculate_true_solar_time(dt, "Malacca", BEIJING_MERIDIAN);
    assert_eq!(sun_time.hour(), 6);
    assert_eq!(sun_time.minute(), 23);
  }

  #[test]
  fn test_to_true_solar_time_defaults_and_conversion() {
    let dt = NaiveDate::from_ymd_opt(1998, 10, 8).and_then(|d| d.and_hms_opt(7, 21, 0)).expect("valid test date");
    assert_eq!(to_true_solar_time(dt, None), dt);
    assert_eq!(to_true_solar_time(dt, Some("UnknownCity")), dt);

    let sun_time = to_true_solar_time(dt, Some("Malacca"));
    assert_eq!(sun_time.hour(), 6);
    assert_eq!(sun_time.minute(), 23);
  }

  #[test]
  fn test_equation_of_time_bounds() {
    for day in 1..=365 {
      let eot = get_equation_of_time(day);
      assert!(eot > -20.0 && eot < 20.0, "EoT out of expected range on day {}: {}", day, eot);
    }
  }
}
