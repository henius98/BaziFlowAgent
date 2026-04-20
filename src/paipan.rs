use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize)]
pub struct BaziChart {
    #[serde(alias = "bz", rename = "八字")]
    pub bz: Value,
    #[serde(alias = "ss", rename = "十神")]
    pub ss: Vec<String>,
    #[serde(alias = "cg", rename = "藏干")]
    pub cg: Vec<Vec<String>>,
    #[serde(alias = "cgss", rename = "藏干十神")]
    pub cgss: Vec<Vec<String>>,
    #[serde(alias = "ny", rename = "纳音")]
    pub ny: Vec<String>,
    #[serde(alias = "szshensha", rename = "四柱神煞")]
    pub szshensha: Vec<Vec<String>>,
    #[serde(alias = "kw", rename = "空亡柱")]
    pub kw: Vec<String>,
    #[serde(alias = "xy", rename = "星运")]
    pub xy: Vec<String>,
    #[serde(alias = "zz", rename = "自坐")]
    pub zz: Vec<String>,
    #[serde(alias = "dayun", rename = "大运")]
    pub dayun: Vec<String>,
    #[serde(alias = "xiaoyun", rename = "小运")]
    pub xiaoyun: Vec<String>,
    #[serde(alias = "dyshensha", rename = "大运神煞")]
    pub dyshensha: Value,
    #[serde(alias = "qiyunsui", rename = "起运岁")]
    pub qiyunsui: i32,
    #[serde(alias = "qiyunarr", rename = "起运时间表")]
    pub qiyunarr: Vec<i32>,
    #[serde(alias = "jiaoyun", rename = "交运时间")]
    pub jiaoyun: String,
    #[serde(alias = "kongwang", rename = "空亡")]
    pub kongwang: String,
    #[serde(alias = "taixi", rename = "胎息")]
    pub taixi: String,
    #[serde(alias = "taiyuan", rename = "胎元")]
    pub taiyuan: String,
    #[serde(alias = "minggong", rename = "命宫")]
    pub minggong: String,
    #[serde(alias = "shenggong", rename = "身宫")]
    pub shenggong: String,
    #[serde(alias = "taixi_nayin", rename = "胎息纳音")]
    pub taixi_nayin: String,
    #[serde(alias = "taiyuan_nayin", rename = "胎元纳音")]
    pub taiyuan_nayin: String,
    #[serde(alias = "minggong_nayin", rename = "命宫纳音")]
    pub minggong_nayin: String,
    #[serde(alias = "shenggong_nayin", rename = "身宫纳音")]
    pub shenggong_nayin: String,
    #[serde(alias = "sex", rename = "性别")]
    pub sex: i32,
    #[serde(default, alias = "lunar_date", rename = "农历")]
    pub lunar_date: String,
}

pub async fn fetch_bazi_chart(
    client: &Client,
    date: &str, // YYYY-MM-DD
    hour: u32,
    minute: u32,
    gender: u8, // 1 for male, 0 for female
) -> crate::logger::AppResult<(BaziChart, String)> {
    let date_str = format!("{} {:02}:{:02}", date, hour, minute);
    let api_url = format!(
        "https://bzapi4.iwzbz.com/getbasebz8.php?d={}&s={}&today=undefined&vip=1&userguid=&yzs=0",
        date_str, gender
    );
    let response = client.get(&api_url).send().await?.error_for_status()?;
    let raw_data: Value = response.json().await?;
    let mut chart: BaziChart = serde_json::from_value(raw_data)?;
    
    if let Some(lunar) = chart.bz.get("8").and_then(|v| v.as_str()) {
        chart.lunar_date = lunar.to_string();
    }

    let structured_data = arrange_bazi_data(&chart);
    let translated_json = serde_json::to_string_pretty(&structured_data)?;
    Ok((chart, translated_json))
}

#[derive(Serialize)]
pub struct PillarData {
    #[serde(rename = "柱名")]
    pub name: String,
    #[serde(rename = "主星")]
    pub main_star: String,
    #[serde(rename = "天干")]
    pub stem: String,
    #[serde(rename = "地支")]
    pub branch: String,
    #[serde(rename = "藏干")]
    pub hidden_stems: Vec<String>,
    #[serde(rename = "副星")]
    pub sub_stars: Vec<String>,
    #[serde(rename = "星运")]
    pub star_luck: String,
    #[serde(rename = "自坐")]
    pub self_sitting: String,
    #[serde(rename = "空亡")]
    pub empty_death: String,
    #[serde(rename = "纳音")]
    pub nayin: String,
    #[serde(rename = "神煞")]
    pub shensha: Vec<String>,
}

#[derive(Serialize)]
pub struct StructuredBazi {
    #[serde(rename = "基本信息")]
    pub info: std::collections::HashMap<String, String>,
    #[serde(rename = "四柱排盘")]
    pub pillars: Vec<PillarData>,
    #[serde(rename = "大运")]
    pub dayun: Vec<String>,
    #[serde(rename = "小运")]
    pub xiaoyun: Vec<String>,
    #[serde(rename = "大运神煞")]
    pub dyshensha: Vec<String>,
    #[serde(rename = "起运信息")]
    pub luck_info: std::collections::HashMap<String, String>,
    #[serde(rename = "其他")]
    pub other: std::collections::HashMap<String, String>,
}

pub fn arrange_bazi_data(chart: &BaziChart) -> StructuredBazi {
    let pillar_names = vec!["年柱", "月柱", "日柱", "时柱"];
    let mut pillars = Vec::new();

    for i in 0..4 {
        let stem_idx = (i * 2).to_string();
        let branch_idx = (i * 2 + 1).to_string();
        
        let stem = chart.bz.get(&stem_idx).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let branch = chart.bz.get(&branch_idx).and_then(|v| v.as_str()).unwrap_or("").to_string();

        pillars.push(PillarData {
            name: pillar_names[i].to_string(),
            main_star: chart.ss.get(i).cloned().unwrap_or_default(),
            stem,
            branch,
            hidden_stems: chart.cg.get(i).cloned().unwrap_or_default(),
            sub_stars: chart.cgss.get(i).cloned().unwrap_or_default(),
            star_luck: chart.xy.get(i).cloned().unwrap_or_default(),
            self_sitting: chart.zz.get(i).cloned().unwrap_or_default(),
            empty_death: chart.kw.get(i).cloned().unwrap_or_default(),
            nayin: chart.ny.get(i).cloned().unwrap_or_default(),
            shensha: chart.szshensha.get(i).cloned().unwrap_or_default(),
        });
    }

    let mut info = std::collections::HashMap::new();
    info.insert("性别".to_string(), if chart.sex == 1 { "男" } else { "女" }.to_string());
    info.insert("农历".to_string(), chart.lunar_date.clone());

    let mut luck_info = std::collections::HashMap::new();
    luck_info.insert("起运岁".to_string(), chart.qiyunsui.to_string());
    luck_info.insert("交运时间".to_string(), chart.jiaoyun.clone());
    luck_info.insert("起运时间表".to_string(), chart.qiyunarr.iter().map(|v| v.to_string()).collect::<Vec<String>>().join(", "));

    let mut dyshensha = Vec::new();
    if let Some(arr) = chart.dyshensha.as_array() {
        for item in arr {
            if let Some(pair) = item.as_array() {
                if pair.len() >= 2 {
                    let luck_pillar = pair[0].as_str().unwrap_or("");
                    let shensha_list = pair[1].as_array().map(|list| {
                        list.iter().filter_map(|s| s.as_str()).collect::<Vec<&str>>().join(", ")
                    }).unwrap_or_default();
                    dyshensha.push(format!("{}: {}", luck_pillar, shensha_list));
                }
            }
        }
    }

    let mut other = std::collections::HashMap::new();
    other.insert("空亡".to_string(), chart.kongwang.clone());
    other.insert("胎息".to_string(), format!("{} ({})", chart.taixi, chart.taixi_nayin));
    other.insert("胎元".to_string(), format!("{} ({})", chart.taiyuan, chart.taiyuan_nayin));
    other.insert("命宫".to_string(), format!("{} ({})", chart.minggong, chart.minggong_nayin));
    other.insert("身宫".to_string(), format!("{} ({})", chart.shenggong, chart.shenggong_nayin));

    StructuredBazi {
        info,
        pillars,
        dayun: chart.dayun.clone(),
        xiaoyun: chart.xiaoyun.clone(),
        dyshensha,
        luck_info,
        other,
    }
}

pub fn format_bazi_for_prompt(chart: &BaziChart) -> String {
    let structured = arrange_bazi_data(chart);
    serde_json::to_string_pretty(&structured).unwrap_or_default()
}
