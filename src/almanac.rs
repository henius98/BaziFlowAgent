use reqwest::Client;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::OnceLock;

static KEY_MAP: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
static KEEP_SCHEMA: OnceLock<Value> = OnceLock::new();

/// English to Chinese translations mapping - built once
fn get_key_map() -> &'static HashMap<&'static str, &'static str> {
    KEY_MAP.get_or_init(|| {
        let mut map = HashMap::new();
        map.insert("data", "data");
        map.insert("bottom", "额外补充");
        map.insert("ganZhi", "干支");
        map.insert("info", "基本信息");
        map.insert("lunar", "农历");
        map.insert("solar", "公历");
        map.insert("yiJi", "宜忌");
        map.insert("positions", "吉神方位");
        map.insert("zodiac", "生肖");
        map.insert("hours", "时辰吉凶");

        map.insert("yi", "宜");
        map.insert("ji", "忌");

        map.insert("jiShen", "吉神宜趋");
        map.insert("xiongSha", "凶煞宜忌");
        map.insert("tianShen", "值神");
        map.insert("taiShen", "今日胎神");

        map.insert("liuYao", "六曜");
        map.insert("xiu", "二十八星宿");
        map.insert("xiuLuck", "星宿吉凶");
        map.insert("yueXiang", "月相");
        map.insert("zhiXing", "建除十二神");
        map.insert("xingZuo", "星座");

        map.insert("cai", "财神");
        map.insert("xi", "喜神");
        map.insert("fu", "福神");
        map.insert("yangGui", "阳贵神");
        map.insert("yinGui", "阴贵神");
        map.insert("dayTai", "逐日胎神");
        map.insert("monthTai", "逐月胎神");
        map.insert("yearTai", "逐年胎神");

        map.insert("chongDesc", "冲煞");
        map.insert("chongShengXiao", "冲生肖");
        map.insert("sha", "煞方");
        map.insert("luck", "吉凶");

        map.insert("year", "年");
        map.insert("month", "月");
        map.insert("day", "日");
        map.insert("time", "时");
        map.insert("weekInChinese", "星期");
        map.insert("dayInChinese", "农历日");
        map.insert("monthInChinese", "农历月");

        map.insert("yearNaYin", "年纳音");
        map.insert("monthNaYin", "月纳音");
        map.insert("dayNaYin", "日纳音");

        map.insert("timeZhi", "时支");
        map.insert("zhi", "地支");

        map
    })
}

/// Define what fields to keep - built once
fn get_keep_schema() -> &'static Value {
    KEEP_SCHEMA.get_or_init(|| {
        json!({
            "solar": false,
            "lunar": true,
            "ganZhi": {
                "year": true,
                "month": true,
                "day": true,
                "time": false,
                "timeZhi": false
            },
            "zodiac": false,
            "yiJi": false,
            "info": true,
            "hours": false,
            "positions": false,
            "bottom": {
                "jiShen": true,
                "taiShen": false,
                "xiu": true,
                "xiuLuck": true,
                "zhiXing": true,
                "liuYao": true,
                "yueXiang": false,
                "xiongSha": true
            }
        })
    })
}

/// Recursively filters 'data' based on 'schema'
fn filter_data(data: &Value, schema: &Value) -> Option<Value> {
    if let Value::Bool(true) = schema {
        return Some(data.clone());
    }
    if let Value::Bool(false) = schema {
        return None;
    }

    if let Value::Array(arr) = data {
        let mut filtered_list = Vec::new();
        for item in arr {
            if let Some(filtered_item) = filter_data(item, schema) {
                filtered_list.push(filtered_item);
            }
        }
        return Some(Value::Array(filtered_list));
    }

    if let Value::Object(data_map) = data {
        if let Value::Object(schema_map) = schema {
            let mut new_data = serde_json::Map::new();
            for (k, sub_schema) in schema_map {
                if let Some(val) = data_map.get(k) {
                    if let Some(filtered_val) = filter_data(val, sub_schema) {
                        new_data.insert(k.clone(), filtered_val);
                    }
                }
            }
            return Some(Value::Object(new_data));
        }
    }

    None
}

/// Calculate "Kong Wang" (空亡)
fn calculate_kong_wang(day_gan_zhi: &str) -> String {
    let heavenly_stems = ["甲", "乙", "丙", "丁", "戊", "己", "庚", "辛", "壬", "癸"];
    let earthly_branches = [
        "子", "丑", "寅", "卯", "辰", "巳", "午", "未", "申", "酉", "戌", "亥",
    ];

    let chars: Vec<char> = day_gan_zhi.chars().collect();
    if chars.len() != 2 {
        return "输入错误：请输入有效的天干和地支".to_string();
    }

    let gan_str = chars[0].to_string();
    let zhi_str = chars[1].to_string();

    let gan_idx = heavenly_stems.iter().position(|&x| x == gan_str);
    let zhi_idx = earthly_branches.iter().position(|&x| x == zhi_str);

    match (gan_idx, zhi_idx) {
        (Some(g), Some(z)) => {
            // Calculate xun start index
            let xun_start_idx = (z as i32 - g as i32).rem_euclid(12) as usize;

            let kw1_idx = (xun_start_idx + 10) % 12;
            let kw2_idx = (xun_start_idx + 11) % 12;

            format!("{}{}", earthly_branches[kw1_idx], earthly_branches[kw2_idx])
        }
        _ => "输入错误：请输入有效的天干和地支".to_string(),
    }
}

/// Translates keys recursively based on KEY_MAP
fn translate_keys(data: Value, key_map: &HashMap<&'static str, &'static str>) -> Value {
    match data {
        Value::Object(map) => {
            let mut new_map = serde_json::Map::new();
            for (k, v) in map {
                let new_key = key_map.get(k.as_str()).unwrap_or(&k.as_str()).to_string();
                new_map.insert(new_key, translate_keys(v, key_map));
            }
            Value::Object(new_map)
        }
        Value::Array(arr) => Value::Array(
            arr.into_iter()
                .map(|v| translate_keys(v, key_map))
                .collect(),
        ),
        other => other,
    }
}

/// Formats Data into a clean string without quotes or newlines
fn to_plaintext(data: &Value) -> String {
    match data {
        Value::Object(map) => {
            let parts: Vec<String> = map
                .iter()
                .map(|(k, v)| format!("{}: {}", k, to_plaintext(v)))
                .collect();
            parts.join(",\n")
        }
        Value::Array(arr) => {
            let parts: Vec<String> = arr.iter().map(|v| to_plaintext(v)).collect();
            parts.join(" ")
        }
        Value::String(s) => s.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
    }
}

pub async fn fetch_and_format_almanac(
    client: &Client,
    target_date: &str,
) -> crate::logger::AppResult<String> {
    let api_url = format!(
        "https://www.mingdecode.com/api/almanac?date={}",
        target_date
    );

    let response = client.get(&api_url).send().await?.error_for_status()?;

    let raw_data: Value = response.json().await?;

    if raw_data.is_object() {
        // Step 1: Filter
        let schema = get_keep_schema();
        let mut filtered_data = filter_data(&raw_data, schema).unwrap_or(Value::Null);

        // Step 2: Calculate Kong Wang
        if let Some(day_gan_zhi) = filtered_data
            .get("ganZhi")
            .and_then(|v| v.get("day"))
            .and_then(|v| v.as_str())
        {
            let kw = calculate_kong_wang(day_gan_zhi);
            if let Value::Object(ref mut map) = filtered_data {
                map.insert("空亡".to_string(), Value::String(kw));
            }
        }

        // Step 3: Translate
        let key_map = get_key_map();
        let final_data = translate_keys(filtered_data, key_map);

        // Step 4: To Plaintext
        Ok(to_plaintext(&final_data))
    } else {
        Ok(to_plaintext(&raw_data))
    }
}
