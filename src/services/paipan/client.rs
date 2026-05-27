use super::bazi_utils::{calculate_gz_info, current_luck_index, fetch_liunian, map_bazi_data};
use super::models::RawBaziChart;
use crate::models::AppResult;
use reqwest::Client;
use serde_json::Value;

const REFERER: &str = "https://pcbz.iwzwh.com/";
const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

pub async fn fetch_bazi_chart(client: &Client, solar_dt: chrono::NaiveDateTime, gender: u8, birth_year: i16) -> AppResult<String> {
    let solar_dt_str = solar_dt.format("%Y-%m-%d %H:%M").to_string();

    let mut chart = fetch_base_bazi(client, &solar_dt_str, gender).await?;

    // Extract the 4 pillars for building query strings
    let gz_str = format!(
        "{}{} {}{} {}{} {}{}",
        chart.bz.year_steam, chart.bz.year_branch, chart.bz.month_steam, chart.bz.month_branch, chart.bz.day_steam, chart.bz.day_branch, chart.bz.hour_steam, chart.bz.hour_branch
    );

    let gz_split_str = format!(
        "{} {} {} {} {} {} {} {}",
        chart.bz.year_steam, chart.bz.year_branch, chart.bz.month_steam, chart.bz.month_branch, chart.bz.day_steam, chart.bz.day_branch, chart.bz.hour_steam, chart.bz.hour_branch
    );

    // Resolve Liu Nian pillar from API or calculate from current year
    let (lunisolar_year, ln_gz) = fetch_liunian();
    let luck_index = current_luck_index(&chart, &birth_year) as usize;
    let dy_str = chart.dayun.get(luck_index).map(|s| s.as_str()).unwrap_or("");

    let dy_gz_str = format!("{} {}", dy_str, gz_str);
    let ln_gz_str = format!("{} {}", ln_gz, gz_str);

    // Concurrently fetch all supplementary data
    let (yongshi, ori_relations, dy_relations, ln_relations, ln_shensha) = tokio::join!(
        fetch_yongshi(client, &solar_dt_str),
        fetch_ganzhi_relations(client, &gz_str),
        fetch_ganzhi_relations(client, &dy_gz_str),
        fetch_ganzhi_relations(client, &ln_gz_str),
        fetch_shensha(client, &ln_gz, &gz_split_str, gender),
    );

    // Exclude the already ori_relations values
    let (dy_relations, ln_relations) = if let Some(ori) = &ori_relations {
        let filter_fn = |rels: Option<Vec<Vec<String>>>| {
            rels.map(|r| {
                r.into_iter()
                    .map(|inner_vec| inner_vec.into_iter().filter(|item| !ori.iter().any(|ori_inner| ori_inner.contains(item))).collect::<Vec<String>>())
                    .filter(|inner_vec| !inner_vec.is_empty())
                    .collect()
            })
        };
        (filter_fn(dy_relations), filter_fn(ln_relations))
    } else {
        (dy_relations, ln_relations)
    };

    chart.yongshi = yongshi;
    chart.ori_gz_relations = ori_relations;
    chart.dy_gz_relations = dy_relations;
    chart.ln_gz_relations = ln_relations;
    chart.ln_shensha = ln_shensha;

    let structured_data = map_bazi_data(&chart, gender, solar_dt_str, &birth_year, &lunisolar_year, &ln_gz, &luck_index);
    let structured_json = serde_json::to_string_pretty(&structured_data)?;
    Ok(structured_json)
}

async fn fetch_base_bazi(client: &Client, solar_dt: &str, gender: u8) -> AppResult<RawBaziChart> {
    let api_url = format!("https://bzapi4.iwzbz.com/getbasebz8.php?d={}&s={}&today=undefined&vip=1&userguid=&yzs=0", solar_dt, gender);
    let response = client.get(&api_url).send().await?.error_for_status()?;
    let raw_data: Value = response.json().await?;
    let chart: RawBaziChart = serde_json::from_value(raw_data)?;
    Ok(chart)
}

async fn fetch_yongshi(client: &Client, solar_dt: &str) -> Option<String> {
    let json = client
        .get("https://bzapi2.iwzbz.com/getRysl.php")
        .query(&[("datestr", &format!("{}:00", solar_dt))])
        .send()
        .await
        .ok()?
        .json::<Value>()
        .await
        .ok()?;

    json.get("data").and_then(|v| v.as_str()).map(|d| d.to_string())
}

async fn fetch_ganzhi_relations(client: &Client, gz_str: &str) -> Option<Vec<Vec<String>>> {
    client
        .get("https://bzapi4.iwzbz.com/getGZRelaction3.php")
        .query(&[("gz", gz_str), ("userguid", ""), ("vip", "0")])
        .send()
        .await
        .ok()?
        .json::<Vec<Vec<String>>>()
        .await
        .ok()
}

// fetch shensha for dayun or liunian
async fn fetch_shensha(client: &Client, ln: &str, gz_str: &str, gender: u8) -> Option<Vec<String>> {
    let sex_str = gender.to_string();
    let json = client
        .get("https://bzapi2.iwzbz.com/getliunianshensha5.php")
        .header("Referer", REFERER)
        .header("User-Agent", USER_AGENT)
        .query(&[("ln", ln), ("bz", gz_str), ("sex", sex_str.as_str()), ("vip", "0"), ("userguid", "")])
        .send()
        .await
        .ok()?
        .json::<Value>()
        .await
        .ok()?;

    let ss_arr = json.get("shensha")?.get(1)?.as_array()?;
    Some(ss_arr.iter().filter_map(|s| s.as_str().map(|s| s.to_string())).collect())
}
