use super::bazi_utils;
use super::models::{AdditionalInfo, BasePillarData, BasicInfo, CurrentLuck, DaYunData, ElementStates, LuckInfo, PillarData, RawBaziChart, Relations, StructuredBazi};
use chrono::Datelike;
use serde_json::Value;

const HTML_TEMPLATE: &str = include_str!("bazi_template.html");

// ─── Shared helpers ──────────────────────────────────────────

/// Build hidden-stem HTML from a gz_info JSON (cg + cgss arrays)
fn hidden_html_from_info(info: &Value) -> String {
    let cg = info.get("cg").and_then(|v| v.as_array());
    let cgss = info.get("cgss").and_then(|v| v.as_array());
    match (cg, cgss) {
        (Some(cg), Some(cgss)) => cg
            .iter()
            .zip(cgss.iter())
            .map(|(s, ss)| format!("<div>{}<span>{}</span></div>", s.as_str().unwrap_or(""), ss.as_str().unwrap_or("")))
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

/// Build `<div>` list from a slice of strings
fn divs_from_slice(items: &[String]) -> String {
    items.iter().map(|s| format!("<div>{}</div>", s)).collect::<Vec<_>>().join("")
}

// /// Calculate Ten God (十神) relationship between day master and target stem
// fn calculate_ten_god(day_master: &str, target_stem: &str) -> String {
//     let stems = ["甲", "乙", "丙", "丁", "戊", "己", "庚", "辛", "壬", "癸"];
//     let dm_idx = stems.iter().position(|&s| s == day_master).unwrap_or(0);
//     let tg_idx = stems.iter().position(|&s| s == target_stem).unwrap_or(0);

//     let same_polarity = dm_idx % 2 == tg_idx % 2;
//     let rel = (tg_idx / 2 + 5 - dm_idx / 2) % 5;

//     match (rel, same_polarity) {
//         (0, true) => "比肩",
//         (0, false) => "劫财",
//         (1, true) => "食神",
//         (1, false) => "伤官",
//         (2, true) => "偏财",
//         (2, false) => "正财",
//         (3, true) => "七杀",
//         (3, false) => "正官",
//         (4, true) => "偏印",
//         (4, false) => "正印",
//         _ => "",
//     }
//     .to_string()
// }

// ─── Public formatters ───────────────────────────────────────

// pub fn format_bazi_for_prompt(chart: &RawBaziChart) -> String {
//     let structured = arrange_bazi_data(chart);
//     serde_json::to_string_pretty(&structured).unwrap_or_default()
// }

// pub fn generate_bazi_html(chart: &RawBaziChart, name: &str) -> String {
//     let data = arrange_bazi_data(chart);
//     let mut html = HTML_TEMPLATE.to_string();

//     // Basic Info
//     html = html.replace("{{NAME}}", name);
//     html = html.replace("{{GENDER_SUFFIX}}", if chart.sex == 1 { "乾造" } else { "坤造" });
//     html = html.replace("{{lunisolar_date}}", &data.info.lunisolar_date);
//     html = html.replace("{{SOLAR_DATE}}", &data.info.solar_date);

//     // Current Luck columns (流年 + 大运)
//     populate_luck_columns(&mut html, &data);

//     // Four natal pillars
//     let prefixes = ["YEAR", "MONTH", "DAY", "HOUR"];
//     for (i, p) in data.pillars.iter().enumerate() {
//         let px = prefixes[i];
//         html = html.replace(&format!("{{{{{}_GOD}}}}", px), &p.main_star);
//         html = html.replace(&format!("{{{{{}_STEM}}}}", px), &p.stem);
//         html = html.replace(&format!("{{{{{}_BRANCH}}}}", px), &p.branch);
//         html = html.replace(&format!("{{{{{}_LUCK}}}}", px), &p.star_luck);
//         html = html.replace(&format!("{{{{{}_ZIZUO}}}}", px), &p.self_sitting);
//         html = html.replace(&format!("{{{{{}_KW}}}}", px), &p.empty_death);
//         html = html.replace(&format!("{{{{{}_NAYIN}}}}", px), &p.nayin);
//         html = html.replace(&format!("{{{{{}_SHENSHA}}}}", px), &divs_from_slice(&p.shensha));
//         let hidden = p
//             .hidden_stems_and_stars
//             .iter()
//             .map(|(s, god)| {
//                 format!("<div class=\"hidden-item\"><span class=\"hidden-stem\">{}</span><span class=\"hidden-god\">{}</span></div>", s, god)
//             })
//             .collect::<Vec<_>>()
//             .join("");
//         html = html.replace(&format!("{{{{{}_HIDDEN}}}}", px), &hidden);
//     }

//     // Interactions
//     let format_relations = |rels: &Option<Vec<String>>| -> String {
//         match rels {
//             Some(items) => items.iter().map(|r| format!("<span>{}</span>", r.split(',').next().unwrap_or(r))).collect::<Vec<_>>().join(", "),
//             None => "无明显关系".to_string(),
//         }
//     };
//     html = html.replace("{{STEM_INTERACTIONS}}", &format_relations(&data.stem_relations));
//     html = html.replace("{{BRANCH_INTERACTIONS}}", &format_relations(&data.branch_relations));

//     html
// }

/*
/// Populate the 流年 and 大运 columns in the HTML template.
fn populate_luck_columns(html: &mut String, data: &StructuredBazi) {
    // All placeholders that need clearing if no luck data
    const LUCK_PLACEHOLDERS: &[&str] = &[
        "YEAR_PILLAR_0",
        "YEAR_PILLAR_1",
        "YEAR_GOD",
        "LUCK_PILLAR_0",
        "LUCK_PILLAR_1",
        "LUCK_GOD",
        "YEAR_SHENSHA_CURRENT",
        "LUCK_SHENSHA_CURRENT",
        "LN_HIDDEN",
        "LN_LUCK",
        "LN_ZIZUO",
        "LN_KW",
        "LN_NAYIN",
        "DY_HIDDEN",
        "DY_LUCK",
        "DY_ZIZUO",
        "DY_KW",
        "DY_NAYIN",
    ];

    let luck = match &data.current_luck {
        Some(l) => l,
        None => {
            for ph in LUCK_PLACEHOLDERS {
                *html = html.replace(&format!("{{{{{}}}}}", ph), "");
            }
            return;
        }
    };

    let year_p = luck.year.as_deref().unwrap_or("");
    let luck_p = luck.active_dayun.as_deref().unwrap_or("");
    let day_master = data.pillars.get(2).map(|p| p.stem.as_str()).unwrap_or("");

    // Extract first char as stem, second as branch
    let year_stem: String = year_p.chars().next().map(|c| c.to_string()).unwrap_or_default();
    let luck_stem: String = luck_p.chars().next().map(|c| c.to_string()).unwrap_or_default();
    let year_branch: String = year_p.chars().nth(1).map(|c| c.to_string()).unwrap_or_default();
    let luck_branch: String = luck_p.chars().nth(1).map(|c| c.to_string()).unwrap_or_default();

    // Ten God labels
    let year_god = if !year_stem.is_empty() && !day_master.is_empty() {
        get_ten_god(day_master, &year_stem)
    } else {
        "流年".to_string()
    };
    let luck_god = if !luck_stem.is_empty() && !day_master.is_empty() {
        get_ten_god(day_master, &luck_stem)
    } else {
        "大运".to_string()
    };

    *html = html.replace("{{YEAR_PILLAR_0}}", &year_stem);
    *html = html.replace("{{YEAR_PILLAR_1}}", &year_branch);
    *html = html.replace("{{YEAR_GOD}}", &year_god);
    *html = html.replace("{{LUCK_PILLAR_0}}", &luck_stem);
    *html = html.replace("{{LUCK_PILLAR_1}}", &luck_branch);
    *html = html.replace("{{LUCK_GOD}}", &luck_god);

    // Shensha
    let ln_shensha = luck
        .shensha
        .as_deref()
        .unwrap_or("")
        .split(", ")
        .filter(|s| !s.is_empty())
        .map(|s| format!("<div>{}</div>", s))
        .collect::<String>();
    *html = html.replace("{{YEAR_SHENSHA_CURRENT}}", &ln_shensha);

    let luck_shensha = data.dayun.iter().find(|d| d.pillar == luck_p).map(|dy| divs_from_slice(&dy.shensha)).unwrap_or_default();
    *html = html.replace("{{LUCK_SHENSHA_CURRENT}}", &luck_shensha);

    // 流年 detail rows from API data
    populate_gz_info(html, "LN", &luck.ln_info);

    // 大运 detail rows from API data
    populate_gz_info(html, "DY", &luck.dy_info);
}

/// Populate HIDDEN/LUCK/ZIZUO/KW/NAYIN placeholders for a given prefix (LN or DY)
/// using the gz_info API response.
fn populate_gz_info(html: &mut String, prefix: &str, info: &Option<Value>) {
    let (hidden, luck, zizuo, kw, nayin) = match info {
        Some(v) => (
            hidden_html_from_info(v),
            json_str(v, "xy").to_string(),
            json_str(v, "zz").to_string(),
            json_str(v, "kw").to_string(),
            json_str(v, "ny").to_string(),
        ),
        None => Default::default(),
    };

    *html = html.replace(&format!("{{{{{}_HIDDEN}}}}", prefix), &hidden);
    *html = html.replace(&format!("{{{{{}_LUCK}}}}", prefix), &luck);
    *html = html.replace(&format!("{{{{{}_ZIZUO}}}}", prefix), &zizuo);
    *html = html.replace(&format!("{{{{{}_KW}}}}", prefix), &kw);
    *html = html.replace(&format!("{{{{{}_NAYIN}}}}", prefix), &nayin);
}
*/
