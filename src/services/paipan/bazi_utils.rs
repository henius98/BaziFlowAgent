//! Shared utility functions for local calculated bazi.

use crate::models::common::{BRANCHES, STATES, STEMS};
use crate::services::paipan::models::{AdditionalInfo, BasePillarData, BasicInfo, CurrentLuck, DaYunData, ElementStates, LuckInfo, PillarData, RawBaziChart, Relations, StructuredBazi};
use chrono::Datelike;

// ─── Structured data assembly ────────────────────────────────
pub fn map_bazi_data(chart: &RawBaziChart, gender: u8, solar_dt_str: String, birth_year: &i32, lunisolar_year: &i32, ln_gz: &str, luck_index: &usize) -> StructuredBazi {
    let pillar_names = ["年柱", "月柱", "日柱", "时柱"];
    let mut pillars = Vec::with_capacity(4);

    for (i, name) in pillar_names.iter().enumerate() {
        let (stem, branch) = match i {
            0 => (&chart.bz.year_steam, &chart.bz.year_branch),
            1 => (&chart.bz.month_steam, &chart.bz.month_branch),
            2 => (&chart.bz.day_steam, &chart.bz.day_branch),
            3 => (&chart.bz.hour_steam, &chart.bz.hour_branch),
            _ => unreachable!(),
        };

        pillars.push(PillarData {
            name: name.to_string(),
            base: BasePillarData {
                stem_and_stars: vec![(stem.to_string(), chart.ss.get(i).cloned().unwrap_or_default())],
                branch: branch.to_string(),
                hidden_stems_and_stars: {
                    let cg = chart.cg.get(i).cloned().unwrap_or_default();
                    let cgss = chart.cgss.get(i).cloned().unwrap_or_default();
                    cg.into_iter().zip(cgss).collect()
                },
                star_luck: chart.xy.get(i).cloned().unwrap_or_default(),
                self_sitting: chart.zz.get(i).cloned().unwrap_or_default(),
                empty_death: chart.kw.get(i).cloned().unwrap_or_default(),
                nayin: chart.ny.get(i).cloned().unwrap_or_default(),
                shensha: chart.szshensha.get(i).cloned().unwrap_or_default(),
            },
        });
    }

    let day_master = &chart.bz.day_steam;
    // Build Da Yun list
    let mut dayun: Vec<DaYunData> = chart
        .dyshensha
        .iter()
        .enumerate()
        .map(|(i, (gz_str, shensha))| {
            let start_year = birth_year + (chart.qiyunsui as i32) + (i as i32) * 10 - 1;
            let is_current_dayun = i == *luck_index;
            let relation = if is_current_dayun { extract_relations(&chart.dy_gz_relations) } else { None };

            DaYunData {
                is_current_dayun,
                start_year,
                end_year: start_year + 9,
                info: calculate_gz_info(gz_str, day_master, shensha.clone()),
                relation,
            }
        })
        .collect();

    // Filter to previous + current + next Da Yun only
    if *luck_index < dayun.len() {
        let end = (*luck_index + 2).min(dayun.len());
        dayun = dayun.drain((*luck_index - 1)..end).collect();
    } else {
        dayun.clear();
    }

    StructuredBazi {
        info: BasicInfo {
            gender: if gender == 1 { "男,乾造" } else { "女,坤造" }.to_string(),
            lunisolar_date: chart.bz.lunisolar_date.clone(),
            solar_date: solar_dt_str,
        },
        pillars,
        other: AdditionalInfo {
            empty_death: chart.kongwang.clone(),
            palace_info: chart.palace_info.clone(),
        },
        relation: extract_relations(&chart.ori_gz_relations),
        yongshi: chart.yongshi.clone(),
        element_states: calculate_element_states(&chart.bz.month_branch),
        luck_info: LuckInfo {
            start_age: chart.qiyunsui.to_string(),
            transition_time: chart.jiaoyun.clone(),
            start_ages: format!("出生后{}年{}月{}天{}时起运", &chart.qiyunarr[0], &chart.qiyunarr[1], &chart.qiyunarr[2], &chart.qiyunarr[3]),
        },
        dayun,
        current_luck: arrange_current_luck(chart, lunisolar_year, ln_gz),
    }
}

pub fn fetch_liunian() -> (i32, String) {
    let now = chrono::Local::now();
    let mut year = now.year();
    // Approximate calculation for LiChun (Start of Spring)
    if now.month() == 1 || (now.month() == 2 && now.day() < 4) {
        year -= 1;
    }
    let stem_idx = (year.rem_euclid(10) + 6) % 10;
    let branch_idx = (year.rem_euclid(12) + 8) % 12;
    let gz = format!("{}{}", STEMS[stem_idx as usize], BRANCHES[branch_idx as usize]);
    (year, gz)
}

/// Find the index of the current active Da Yun based on age
pub fn current_luck_index(chart: &RawBaziChart, birth_year: &i32) -> u8 {
    let age = chrono::Local::now().year() - birth_year + 1;
    let diff = age - chart.qiyunsui as i32;
    (diff / 10).max(0) as u8
}

fn extract_relations(relations: &Option<Vec<Vec<String>>>) -> Option<Relations> {
    fn clean_relations(v: &[String]) -> Vec<String> {
        v.iter()
            .filter(|s| !s.is_empty())
            .map(|s| s.split(',').next().map(|first| first.to_string()).unwrap_or_else(|| s.clone()))
            .collect()
    }
    relations.as_ref().map(|rel| Relations {
        stem_relations: rel.first().map(|v| clean_relations(v)).filter(|v| !v.is_empty()),
        branch_relations: rel.get(1).map(|v| clean_relations(v)).filter(|v| !v.is_empty()),
    })
}

/// Locally calculate detailed pillar info (Hidden Stems, Ten Gods, Star Luck, Empty-death, Na Yin)
/// for any Gan-Zhi pair. Used for both Liu Nian and Da Yun columns.
pub fn calculate_gz_info(gz: &str, day_master: &str, shensha: Vec<String>) -> BasePillarData {
    let stem = gz.chars().next().map(|c| c.to_string()).unwrap_or_default();
    let branch = gz.chars().nth(1).map(|c| c.to_string()).unwrap_or_default();

    if stem.is_empty() || branch.is_empty() {
        return BasePillarData {
            stem_and_stars: vec![],
            branch: String::new(),
            hidden_stems_and_stars: vec![],
            star_luck: String::new(),
            self_sitting: String::new(),
            empty_death: String::new(),
            nayin: String::new(),
            shensha,
        };
    }

    BasePillarData {
        stem_and_stars: vec![(stem.clone(), get_ten_god(day_master, &stem).to_string())],
        branch: branch.clone(),
        hidden_stems_and_stars: get_hidden_stems(&branch).iter().map(|&s| (s.to_string(), get_ten_god(day_master, s).to_string())).collect(),
        star_luck: get_star_luck(day_master, &branch).to_string(),
        self_sitting: get_star_luck(&stem, &branch).to_string(),
        empty_death: get_empty_death(&stem, &branch),
        nayin: get_nayin(gz).to_string(),
        shensha,
    }
}

fn calculate_element_states(month_branch: &str) -> ElementStates {
    // Order: wood, fire, earth, metal, water → mapped to 旺相休囚死 cycle
    let [w, f, e, m, wa] = match month_branch {
        "寅" | "卯" => ["旺", "相", "死", "囚", "休"],
        "巳" | "午" => ["休", "旺", "相", "死", "囚"],
        "申" | "酉" => ["死", "囚", "休", "旺", "相"],
        "亥" | "子" => ["相", "死", "囚", "休", "旺"],
        _ => ["囚", "休", "旺", "相", "死"], // 辰戌丑未 (earth season)
    };
    ElementStates {
        wood: w.to_string(),
        fire: f.to_string(),
        earth: e.to_string(),
        metal: m.to_string(),
        water: wa.to_string(),
    }
}

fn arrange_current_luck(chart: &RawBaziChart, lunisolar_year: &i32, ln_gz: &str) -> Option<CurrentLuck> {
    let day_master = &chart.bz.day_steam;
    let shensha = chart.ln_shensha.clone().unwrap_or_default();

    Some(CurrentLuck {
        year: *lunisolar_year,
        info: calculate_gz_info(ln_gz, day_master, shensha),
        relation: extract_relations(&chart.ln_gz_relations),
    })
}

pub fn get_ten_god(day_master: &str, target_stem: &str) -> &'static str {
    let dm_idx = STEMS.iter().position(|&s| s == day_master).unwrap_or(0);
    let tg_idx = STEMS.iter().position(|&s| s == target_stem).unwrap_or(0);

    let same_polarity = dm_idx % 2 == tg_idx % 2;
    let rel = (tg_idx as i32 / 2 + 5 - dm_idx as i32 / 2).rem_euclid(5);

    match (rel, same_polarity) {
        (0, true) => "比肩",
        (0, false) => "劫财",
        (1, true) => "食神",
        (1, false) => "伤官",
        (2, true) => "偏财",
        (2, false) => "正财",
        (3, true) => "七杀",
        (3, false) => "正官",
        (4, true) => "偏印",
        (4, false) => "正印",
        _ => "",
    }
}

pub fn get_hidden_stems(branch: &str) -> Vec<&'static str> {
    match branch {
        "子" => vec!["癸"],
        "丑" => vec!["己", "癸", "辛"],
        "寅" => vec!["甲", "丙", "戊"],
        "卯" => vec!["乙"],
        "辰" => vec!["戊", "乙", "癸"],
        "巳" => vec!["丙", "庚", "戊"],
        "午" => vec!["丁", "己"],
        "未" => vec!["己", "丁", "乙"],
        "申" => vec!["庚", "壬", "戊"],
        "酉" => vec!["辛"],
        "戌" => vec!["戊", "辛", "丁"],
        "亥" => vec!["壬", "甲"],
        _ => vec![],
    }
}

pub fn get_star_luck(stem: &str, branch: &str) -> &'static str {
    let stem_idx = STEMS.iter().position(|&s| s == stem).unwrap_or(0);
    let branch_idx = BRANCHES.iter().position(|&s| s == branch).unwrap_or(0);

    let start_idx = match stem_idx {
        0 => 11, // 甲 -> 亥
        1 => 6,  // 乙 -> 午
        2 => 2,  // 丙 -> 寅
        3 => 9,  // 丁 -> 酉
        4 => 2,  // 戊 -> 寅
        5 => 9,  // 己 -> 酉
        6 => 5,  // 庚 -> 巳
        7 => 0,  // 辛 -> 子
        8 => 8,  // 壬 -> 申
        9 => 3,  // 癸 -> 卯
        _ => 0,
    };

    let is_yin = stem_idx % 2 != 0;

    let dist = if is_yin {
        (start_idx - branch_idx as i32).rem_euclid(12)
    } else {
        (branch_idx as i32 - start_idx).rem_euclid(12)
    };

    STATES[dist as usize]
}

pub fn get_empty_death(stem: &str, branch: &str) -> String {
    let stem_idx = STEMS.iter().position(|&s| s == stem).unwrap_or(0);
    let branch_idx = BRANCHES.iter().position(|&s| s == branch).unwrap_or(0);

    let e = (branch_idx as i32 + 10 - stem_idx as i32).rem_euclid(12) as usize;
    let next_e = (e + 1) % 12;

    format!("{}{}", BRANCHES[e], BRANCHES[next_e])
}

pub fn get_nayin(gz: &str) -> &'static str {
    match gz {
        "甲子" | "乙丑" => "海中金",
        "丙寅" | "丁卯" => "炉中火",
        "戊辰" | "己巳" => "大林木",
        "庚午" | "辛未" => "路旁土",
        "壬申" | "癸酉" => "剑锋金",
        "甲戌" | "乙亥" => "山头火",
        "丙子" | "丁丑" => "涧下水",
        "戊寅" | "己卯" => "城头土",
        "庚辰" | "辛巳" => "白蜡金",
        "壬午" | "癸未" => "杨柳木",
        "甲申" | "乙酉" => "泉中水",
        "丙戌" | "丁亥" => "屋上土",
        "戊子" | "己丑" => "霹雳火",
        "庚寅" | "辛卯" => "松柏木",
        "壬辰" | "癸巳" => "长流水",
        "甲午" | "乙未" => "沙中金",
        "丙申" | "丁酉" => "山下火",
        "戊戌" | "己亥" => "平地木",
        "庚子" | "辛丑" => "壁上土",
        "壬寅" | "癸卯" => "金箔金",
        "甲辰" | "乙巳" => "覆灯火",
        "丙午" | "丁未" => "天河水",
        "戊申" | "己酉" => "大驿土",
        "庚戌" | "辛亥" => "钗钏金",
        "壬子" | "癸丑" => "桑柘木",
        "甲寅" | "乙卯" => "大溪水",
        "丙辰" | "丁巳" => "沙中土",
        "戊午" | "己未" => "天上火",
        "庚申" | "辛酉" => "石榴木",
        "壬戌" | "癸亥" => "大海水",
        _ => "",
    }
}
