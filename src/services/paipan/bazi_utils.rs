//! Shared utility functions for local calculated bazi.

// ─── Structured data assembly ────────────────────────────────
// Build Da Yun list
// Filter to previous + current + next Da Yun only
// Compute 原局整柱 traits (盖头, 截脚, 伏吟, 反吟)
// Approximate calculation for LiChun (Start of Spring)
/// Find the index of the current active Da Yun based on age
/// Locally calculate detailed pillar info (Hidden Stems, Ten Gods, Star Luck, Empty-death, Na Yin)
/// for any Gan-Zhi pair. Used for both Liu Nian and Da Yun columns.
// Order: wood, fire, earth, metal, water → mapped to 旺相休囚死 cycle
// ─── pillar trait analysis ────────
/// Compute the 原局整柱 summary string for all four natal pillars.
/// Produces entries like: "戊子盖头", "戊寅截脚", "乙卯辛酉反吟"
// Per-pillar: 盖头 / 截脚 / 比和 / 相生
// Cross-pillar pairs: 伏吟 / 反吟
/// Map a Heavenly Stem (天干) character to its Five Element
/// Map an Earthly Branch (地支) character to its Five Element
/// 盖头 (Gaitou): Stem element destroys Branch element (e.g. 庚寅 Metal克Wood)
/// 截脚 (Jiejiao): Branch element destroys Stem element (e.g. 甲申 Metal克Wood from below)
/// 比和 (Bihe): Stem and Branch have the same element
/// 相生 (Xiangsheng): Stem generates Branch OR Branch generates Stem
/// 伏吟 (Fuyin): Two pillars are identical (same stem + same branch)
/// 反吟 (Fanyin): Two pillars clash (mutual elemental destruction on both stem and branch)
pub use baziflow_core::bazi_utils::*;
