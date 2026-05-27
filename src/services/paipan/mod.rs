pub mod client;
pub mod formatter;
pub mod models;
pub mod bazi_utils;

pub use client::fetch_bazi_chart;
// pub use formatter::{format_bazi_for_prompt, generate_bazi_html};
pub use models::{RawBaziChart, StructuredBazi};
