/// Default reference meridian for China / Singapore standard time (UTC+8).
/// Converts Standard Time to True Solar Time (真太阳时) for an optional location.
///
/// If location is None or not found in known cities, returns origin_time unmodified.
/// Uses the Beijing standard meridian (120.0°E) by default.
/// Converts Standard Time (usually Beijing Time / UTC+8) to True Solar Time (真太阳时).
// 1. Longitude Adjustment: 4 minutes per degree
// 2. Equation of Time Adjustment
// Total adjustment in seconds
/// Calculates the Equation of Time (EoT) in minutes for a given day of the year.
/// This uses a standard approximation formula.
pub use baziflow_core::solar_time::*;
