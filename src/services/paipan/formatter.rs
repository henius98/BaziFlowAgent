use super::models::{BasePillarData, StructuredBazi};
use std::fmt;

impl fmt::Display for StructuredBazi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // 基本信息
        writeln!(f, "性别:{}", self.info.gender)?;
        writeln!(f, "出生地:{}", self.info.birth_location)?;
        writeln!(f, "农历:{}", self.info.lunisolar_date)?;
        writeln!(f, "阳历:{}", self.info.solar_date)?;

        // 四柱
        let pillars_str: Vec<String> = self
            .pillars
            .iter()
            .map(|p| {
                let stems: Vec<String> = p.base.stem_and_stars.iter().map(|(s, _)| s.clone()).collect();
                format!("{}: {}{}", p.name, stems.join(""), p.base.branch)
            })
            .collect();
        writeln!(f, "四柱八字:{}", pillars_str.join(","))?;
        // 原局整柱 (盖头, 截脚, 伏吟, 反吟)
        if !self.pillar_traits.is_empty() {
            writeln!(f, "原局整柱:{}", self.pillar_traits.join(", "))?;
        }

        // 主星
        writeln!(f, "- 主星")?;
        let main_stars_str: Vec<String> = self
            .pillars
            .iter()
            .map(|p| {
                let stars: Vec<String> = p.base.stem_and_stars.iter().map(|(_, g)| g.clone()).collect();
                format!("{}: {}", p.name, stars.join(", "))
            })
            .collect();
        writeln!(f, "  {}", main_stars_str.join("; "))?;

        // 藏干
        writeln!(f, "- 藏干")?;
        let hidden_str: Vec<String> = self
            .pillars
            .iter()
            .map(|p| {
                let stems: Vec<String> = p.base.hidden_stems_and_stars.iter().map(|(s, _)| s.clone()).collect();
                format!("{}: {}", p.name, stems.join(", "))
            })
            .collect();
        writeln!(f, "  {}", hidden_str.join("; "))?;

        // 副星
        writeln!(f, "- 副星")?;
        let stars_str: Vec<String> = self
            .pillars
            .iter()
            .map(|p| {
                let stars: Vec<String> = p.base.hidden_stems_and_stars.iter().map(|(_, g)| g.clone()).collect();
                format!("{}: {}", p.name, stars.join(", "))
            })
            .collect();
        writeln!(f, "  {}", stars_str.join("; "))?;

        // 星运
        writeln!(f, "- 星运")?;
        let lucks_str: Vec<String> = self.pillars.iter().map(|p| format!("{}: {}", p.name, p.base.star_luck)).collect();
        writeln!(f, "  {}", lucks_str.join("; "))?;

        // 自坐
        writeln!(f, "- 自坐")?;
        let self_sitting_str: Vec<String> = self.pillars.iter().map(|p| format!("{}: {}", p.name, p.base.self_sitting)).collect();
        writeln!(f, "  {}", self_sitting_str.join("; "))?;

        // 空亡
        writeln!(f, "- 空亡")?;
        let kw_str: Vec<String> = self.pillars.iter().map(|p| format!("{}: {}", p.name, p.base.empty_death)).collect();
        writeln!(f, "  {}", kw_str.join("; "))?;

        // 纳音
        writeln!(f, "- 纳音")?;
        let nayin_str: Vec<String> = self.pillars.iter().map(|p| format!("{}: {}", p.name, p.base.nayin)).collect();
        writeln!(f, "  {}", nayin_str.join("; "))?;

        // 神煞
        writeln!(f, "- 神煞")?;
        let shensha_str: Vec<String> = self.pillars.iter().map(|p| format!("{}: {}", p.name, p.base.shensha.join(", "))).collect();
        writeln!(f, "  {}", shensha_str.join("; "))?;

        // 三垣
        writeln!(f, "- 三垣")?;
        let pi = &self.other.palace_info;
        writeln!(f, "  胎元: {}; 纳音: {}", pi.fetal_origin, pi.fetal_origin_nayin)?;
        writeln!(f, "  命宫: {}; 纳音: {}", pi.life_palace, pi.life_palace_nayin)?;
        writeln!(f, "  身宫: {}; 纳音: {}", pi.body_palace, pi.body_palace_nayin)?;

        // 五行旺衰
        let e = &self.element_states;
        writeln!(f, "五行旺衰: 木{}, 火{}, 土{}, 金{}, 水{}", e.wood, e.fire, e.earth, e.metal, e.water)?;

        // 原局关系
        if let Some(r) = &self.relation {
            if let Some(sr) = &r.stem_relations {
                writeln!(f, "原局天干: {}", sr.join(", "))?;
            }
            if let Some(br) = &r.branch_relations {
                writeln!(f, "原局地支: {}", br.join(", "))?;
            }
        }

        // 用事
        if let Some(y) = &self.yongshi {
            writeln!(f, "用事: {}", y)?;
        }

        // 起运信息
        let li = &self.luck_info;
        writeln!(f, "起运岁: {}", li.start_age)?;
        writeln!(f, "交运时间: {}", li.transition_time)?;
        writeln!(f, "起运时间表: {}", li.start_ages)?;

        // 大运 (all)
        for dy in &self.dayun {
            let current_marker = if dy.is_current_dayun { " (当前)" } else { "" };
            writeln!(f, "- 大运 {}年~{}年{}", dy.start_year, dy.end_year, current_marker)?;
            write_pillar_detail(f, &dy.info)?;
            if let Some(r) = &dy.relation {
                if let Some(sr) = &r.stem_relations {
                    writeln!(f, "  天干关系: {}", sr.join(", "))?;
                }
                if let Some(br) = &r.branch_relations {
                    writeln!(f, "  地支关系: {}", br.join(", "))?;
                }
            }
        }

        // 流年
        if let Some(luck) = &self.liunian {
            writeln!(f, "- 流年 {}年", luck.year)?;
            write_pillar_detail(f, &luck.info)?;
            if let Some(r) = &luck.relation {
                if let Some(sr) = &r.stem_relations {
                    writeln!(f, "  天干关系: {}", sr.join(", "))?;
                }
                if let Some(br) = &r.branch_relations {
                    writeln!(f, "  地支关系: {}", br.join(", "))?;
                }
            }
        }

        Ok(())
    }
}

fn write_pillar_detail(f: &mut fmt::Formatter<'_>, info: &BasePillarData) -> fmt::Result {
    let stems: Vec<String> = info.stem_and_stars.iter().map(|(s, _)| s.clone()).collect();
    let gods: Vec<String> = info.stem_and_stars.iter().map(|(_, g)| g.clone()).collect();
    writeln!(f, "  主星: {}", gods.join(", "))?;
    writeln!(f, "  天干: {}", stems.join(", "))?;
    writeln!(f, "  地支: {}", info.branch)?;
    let hidden: Vec<String> = info.hidden_stems_and_stars.iter().map(|(s, g)| format!("{} {}", s, g)).collect();
    writeln!(f, "  藏干: {}", hidden.join(", "))?;
    writeln!(f, "  星运: {}", info.star_luck)?;
    writeln!(f, "  自坐: {}", info.self_sitting)?;
    writeln!(f, "  空亡: {}", info.empty_death)?;
    writeln!(f, "  纳音: {}", info.nayin)?;
    writeln!(f, "  神煞: {}", info.shensha.join(", "))
}

// ─── Public formatters ───────────────────────────────────────
pub fn generate_bazi_html(chart: &StructuredBazi, name: &str) -> String {
    let mut html = include_str!("bazi_template.html").to_string();

    // Basic Info
    html = html.replace("{{NAME}}", name);
    html = html.replace("{{GENDER_SUFFIX}}", &chart.info.gender);
    html = html.replace("{{lunisolar_date}}", &chart.info.lunisolar_date);
    html = html.replace("{{SOLAR_DATE}}", &chart.info.solar_date);

    // Current Luck columns (流年 + 大运)
    populate_luck_columns(&mut html, chart);

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
    let format_relations = |items: &[String]| -> String {
        if items.is_empty() {
            "无明显关系".to_string()
        } else {
            items.iter().map(|r| format!("<span>{}</span>", r.split(',').next().unwrap_or(r))).collect::<Vec<_>>().join(", ")
        }
    };

    let mut stem_rels = Vec::new();
    let mut branch_rels = Vec::new();

    // Natal relations
    if let Some(r) = &chart.relation {
        if let Some(sr) = &r.stem_relations {
            stem_rels.extend(sr.iter().map(|s| format!("<i>(原局)</i>{}", s)));
        }
        if let Some(br) = &r.branch_relations {
            branch_rels.extend(br.iter().map(|s| format!("<i>(原局)</i>{}", s)));
        }
    }

    // DaYun relations
    if let Some(r) = chart.dayun.iter().find(|d| d.is_current_dayun).and_then(|d| d.relation.as_ref()) {
        if let Some(sr) = &r.stem_relations {
            stem_rels.extend(sr.iter().map(|s| format!("<i>(大运)</i>{}", s)));
        }
        if let Some(br) = &r.branch_relations {
            branch_rels.extend(br.iter().map(|s| format!("<i>(大运)</i>{}", s)));
        }
    }

    // Current luck relations
    if let Some(r) = chart.liunian.as_ref().and_then(|l| l.relation.as_ref()) {
        if let Some(sr) = &r.stem_relations {
            stem_rels.extend(sr.iter().map(|s| format!("<i>(流年)</i>{}", s)));
        }
        if let Some(br) = &r.branch_relations {
            branch_rels.extend(br.iter().map(|s| format!("<i>(流年)</i>{}", s)));
        }
    }

    html = html.replace("{{STEM_INTERACTIONS}}", &format_relations(&stem_rels));
    html = html.replace("{{BRANCH_INTERACTIONS}}", &format_relations(&branch_rels));

    let pillar_traits_html = if !chart.pillar_traits.is_empty() {
        format!(
            "<div class=\"interaction-row\">\n<span class=\"interaction-label\">原局整柱</span>\n<div class=\"interaction-content\">{}</div>\n        </div>",
            chart.pillar_traits.join(", ")
        )
    } else {
        String::new()
    };
    html = html.replace("{{PILLAR_TRAITS_ROW}}", &pillar_traits_html);

    html
}

/// Build `<div>` list from a slice of strings
fn divs_from_slice(items: &[String]) -> String {
    items.iter().map(|s| format!("<div>{}</div>", s)).collect::<Vec<_>>().join("")
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

    let luck = match &data.liunian {
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
