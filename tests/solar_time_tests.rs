use baziflow_agent::services::solar_time::calculate_true_solar_time;
use chrono::{NaiveDate, Timelike};

#[test]
fn test_malacca_conversion() {
    let dt = NaiveDate::from_ymd_opt(1998, 10, 8).and_then(|d| d.and_hms_opt(7, 21, 0)).expect("test constant must be valid");
    let sun_time = calculate_true_solar_time(dt, &"Malacca".to_string(), 120.0);

    // Expected: ~06:22
    // Longitude Adj: (102.25 - 120) * 4 = -71 mins
    // EoT for Oct 8 (day 281): ~ +12 mins
    // Total: -71 + 12 = -59 mins
    // 07:21 - 59 mins = 06:22
    assert_eq!(sun_time.hour(), 6);
    assert_eq!(sun_time.minute(), 22);
}
