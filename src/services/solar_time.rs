use crate::models::common::COMMON_CITIES;
use chrono::{Datelike, Duration, NaiveDateTime};

/// Converts Standard Time (usually Beijing Time / UTC+8) to True Solar Time (真太阳时).
pub fn calculate_true_solar_time(origin_time: NaiveDateTime, city_name: &String, standard_meridian: f64) -> NaiveDateTime {
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
