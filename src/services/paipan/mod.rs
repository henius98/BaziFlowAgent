pub mod bazi_utils;
pub mod client;
pub mod formatter;
pub mod models;

pub use client::fetch_bazi_chart;
pub use formatter::generate_bazi_html;
pub use models::StructuredBazi;
