// 基本信息
// 四柱
// 原局整柱 (盖头, 截脚, 伏吟, 反吟)
// 主星
// 藏干
// 副星
// 星运
// 自坐
// 空亡
// 纳音
// 神煞
// 三垣
// 五行旺衰
// 起运
// 大运
// 流年
// Base HTML template
// Basic Info
// Natal 4 Pillars
// Interactions
// Natal relations
// DaYun relations
// Current luck relations
/// Build `<div>` list from a slice of strings
/// Populate the 流年 and 大运 columns in the HTML template.
// All placeholders that need clearing if no luck data
// Shensha
// 流年 detail rows from BasePillarData
// 大运 detail rows from BasePillarData
/// Populate HIDDEN/LUCK/ZIZUO/KW/NAYIN placeholders for a given prefix (LN or DY)
pub use baziflow_core::formatter::*;
