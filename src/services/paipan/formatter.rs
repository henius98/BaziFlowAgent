use super::models::{BasePillarData, StructuredBazi};

const HTML_TEMPLATE: &str = include_str!("bazi_template.html");

/// Build `<div>` list from a slice of strings
fn divs_from_slice(items: &[String]) -> String {
    items.iter().map(|s| format!("<div>{}</div>", s)).collect::<Vec<_>>().join("")
}

// ─── Public formatters ───────────────────────────────────────

pub fn generate_bazi_html(chart: &StructuredBazi, name: &str) -> String {
    let mut html = HTML_TEMPLATE.to_string();

    // Basic Info
    html = html.replace("{{NAME}}", name);
    html = html.replace("{{GENDER_SUFFIX}}", &chart.info.gender);
    html = html.replace("{{lunisolar_date}}", &chart.info.lunisolar_date);
    html = html.replace("{{SOLAR_DATE}}", &chart.info.solar_date);

    // Current Luck columns (流年 + 大运)
    populate_luck_columns(&mut html, chart); // TODO: Uncomment and update when populate_luck_columns is fixed

    // Four natal pillars
    let prefixes = ["YEAR", "MONTH", "DAY", "HOUR"];
    for (i, p) in chart.pillars.iter().enumerate() {
        if i >= prefixes.len() {
            break;
        }
        let px = prefixes[i];

        let (stem, god) = p.base.stem_and_stars.first().map(|(s, g)| (s.as_str(), g.as_str())).unwrap_or(("", ""));

        html = html.replace(&format!("{{{{{}_GOD}}}}", px), god);
        html = html.replace(&format!("{{{{{}_STEM}}}}", px), stem);
        html = html.replace(&format!("{{{{{}_BRANCH}}}}", px), &p.base.branch);
        html = html.replace(&format!("{{{{{}_LUCK}}}}", px), &p.base.star_luck);
        html = html.replace(&format!("{{{{{}_ZIZUO}}}}", px), &p.base.self_sitting);
        html = html.replace(&format!("{{{{{}_KW}}}}", px), &p.base.empty_death);
        html = html.replace(&format!("{{{{{}_NAYIN}}}}", px), &p.base.nayin);
        html = html.replace(&format!("{{{{{}_SHENSHA}}}}", px), &divs_from_slice(&p.base.shensha));
        let hidden = p
            .base
            .hidden_stems_and_stars
            .iter()
            .map(|(s, god)| format!("<div class=\"hidden-item\"><span class=\"hidden-stem\">{}</span><span class=\"hidden-god\">{}</span></div>", s, god))
            .collect::<Vec<_>>()
            .join("");
        html = html.replace(&format!("{{{{{}_HIDDEN}}}}", px), &hidden);
    }

    // Interactions
    let format_relations = |rels: &Option<Vec<String>>| -> String {
        match rels {
            Some(items) => items.iter().map(|r| format!("<span>{}</span>", r.split(',').next().unwrap_or(r))).collect::<Vec<_>>().join(", "),
            None => "无明显关系".to_string(),
        }
    };
    let stem_relations = chart.relation.as_ref().and_then(|r| r.stem_relations.clone());
    let branch_relations = chart.relation.as_ref().and_then(|r| r.branch_relations.clone());
    html = html.replace("{{STEM_INTERACTIONS}}", &format_relations(&stem_relations));
    html = html.replace("{{BRANCH_INTERACTIONS}}", &format_relations(&branch_relations));

    html
}

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

    let year_stem = luck.info.stem_and_stars.first().map(|s| s.0.as_str()).unwrap_or("");
    let year_god = luck.info.stem_and_stars.first().map(|s| s.1.as_str()).unwrap_or("流年");
    let year_branch = luck.info.branch.as_str();

    let active_dy = data.dayun.iter().find(|d| d.is_current_dayun);
    let (luck_stem, luck_branch, luck_god) = match active_dy {
        Some(dy) => {
            let s = dy.info.stem_and_stars.first().map(|s| s.0.as_str()).unwrap_or("");
            let g = dy.info.stem_and_stars.first().map(|s| s.1.as_str()).unwrap_or("大运");
            let b = dy.info.branch.as_str();
            (s, b, g)
        }
        None => ("", "", ""),
    };

    *html = html.replace("{{YEAR_PILLAR_0}}", year_stem);
    *html = html.replace("{{YEAR_PILLAR_1}}", year_branch);
    *html = html.replace("{{YEAR_GOD}}", year_god);
    *html = html.replace("{{LUCK_PILLAR_0}}", luck_stem);
    *html = html.replace("{{LUCK_PILLAR_1}}", luck_branch);
    *html = html.replace("{{LUCK_GOD}}", luck_god);

    // Shensha
    let ln_shensha = divs_from_slice(&luck.info.shensha);
    *html = html.replace("{{YEAR_SHENSHA_CURRENT}}", &ln_shensha);

    let luck_shensha = active_dy.map(|dy| divs_from_slice(&dy.info.shensha)).unwrap_or_default();
    *html = html.replace("{{LUCK_SHENSHA_CURRENT}}", &luck_shensha);

    // 流年 detail rows from BasePillarData
    populate_gz_info(html, "LN", Some(&luck.info));

    // 大运 detail rows from BasePillarData
    populate_gz_info(html, "DY", active_dy.map(|dy| &dy.info));
}

/// Populate HIDDEN/LUCK/ZIZUO/KW/NAYIN placeholders for a given prefix (LN or DY)
/// using BasePillarData.
fn populate_gz_info(html: &mut String, prefix: &str, info: Option<&BasePillarData>) {
    let (hidden, luck, zizuo, kw, nayin) = match info {
        Some(v) => {
            let h = v
                .hidden_stems_and_stars
                .iter()
                .map(|(s, god)| format!("<div class=\"hidden-item\"><span class=\"hidden-stem\">{}</span><span class=\"hidden-god\">{}</span></div>", s, god))
                .collect::<Vec<_>>()
                .join("");
            (h, v.star_luck.clone(), v.self_sitting.clone(), v.empty_death.clone(), v.nayin.clone())
        }
        None => Default::default(),
    };

    *html = html.replace(&format!("{{{{{}_HIDDEN}}}}", prefix), &hidden);
    *html = html.replace(&format!("{{{{{}_LUCK}}}}", prefix), &luck);
    *html = html.replace(&format!("{{{{{}_ZIZUO}}}}", prefix), &zizuo);
    *html = html.replace(&format!("{{{{{}_KW}}}}", prefix), &kw);
    *html = html.replace(&format!("{{{{{}_NAYIN}}}}", prefix), &nayin);
}
