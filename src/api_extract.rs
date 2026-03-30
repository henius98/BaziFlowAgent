use serde_json::{json, Value};
use std::collections::HashMap;
use tracing::{error, info};

// 1. MAPPING DICTIONARY (English -> Chinese)
fn build_key_map() -> HashMap<&'static str, &'static str> {
    let mut m = HashMap::new();
    // Sections
    m.insert("data", "data");
    m.insert("bottom", "额外补充");
    m.insert("ganZhi", "干支");
    m.insert("info", "基本信息");
    m.insert("lunar", "农历");
    m.insert("solar", "公历");
    m.insert("yiJi", "宜忌");
    m.insert("positions", "吉神方位");
    m.insert("zodiac", "生肖");
    m.insert("hours", "时辰吉凶");

    // Specific Fields - YiJi (Suit/Avoid)
    m.insert("yi", "宜");
    m.insert("ji", "忌");

    // Specific Fields - Shen (Gods/Luck)
    m.insert("jiShen", "吉神宜趋");
    m.insert("xiongSha", "凶煞宜忌");
    m.insert("tianShen", "值神");
    m.insert("taiShen", "今日胎神");

    // Specific Fields - Astrology/Physics
    m.insert("liuYao", "六曜");
    m.insert("xiu", "二十八星宿");
    m.insert("xiuLuck", "星宿吉凶");
    m.insert("yueXiang", "月相");
    m.insert("zhiXing", "建除十二神");
    m.insert("xingZuo", "星座");

    // Specific Fields - Positions
    m.insert("cai", "财神");
    m.insert("xi", "喜神");
    m.insert("fu", "福神");
    m.insert("yangGui", "阳贵神");
    m.insert("yinGui", "阴贵神");
    m.insert("dayTai", "逐日胎神");
    m.insert("monthTai", "逐月胎神");
    m.insert("yearTai", "逐年胎神");

    // Specific Fields - Clash/Details
    m.insert("chongDesc", "冲煞");
    m.insert("chongShengXiao", "冲生肖");
    m.insert("sha", "煞方");
    m.insert("luck", "吉凶");

    // Dates & Time
    m.insert("year", "年");
    m.insert("month", "月");
    m.insert("day", "日");
    m.insert("time", "时");
    m.insert("weekInChinese", "星期");
    m.insert("dayInChinese", "农历日");
    m.insert("monthInChinese", "农历月");

    // NaYin (Sound)
    m.insert("yearNaYin", "年纳音");
    m.insert("monthNaYin", "月纳音");
    m.insert("dayNaYin", "日纳音");

    // Pillars
    m.insert("timeZhi", "时支");
    m.insert("zhi", "地支");

    m
}

/// 2. FILTER CONFIGURATION (Whitelist)
/// Only keys defined here will be kept.
/// `KeepField::Yes` = keep everything, `KeepField::No` = discard,
/// `KeepField::Nested(...)` = keep only specified sub-keys.
#[derive(Clone)]
enum KeepField {
    Yes,
    No,
    Nested(HashMap<&'static str, KeepField>),
}

fn build_keep_fields() -> HashMap<&'static str, KeepField> {
    let mut m = HashMap::new();
    m.insert("solar", KeepField::No);
    m.insert("lunar", KeepField::Yes);

    let mut gan_zhi = HashMap::new();
    gan_zhi.insert("year", KeepField::Yes);
    gan_zhi.insert("month", KeepField::Yes);
    gan_zhi.insert("day", KeepField::Yes);
    gan_zhi.insert("time", KeepField::No);
    gan_zhi.insert("timeZhi", KeepField::No);
    m.insert("ganZhi", KeepField::Nested(gan_zhi));

    m.insert("zodiac", KeepField::No);
    m.insert("yiJi", KeepField::No);
    m.insert("info", KeepField::Yes);
    m.insert("hours", KeepField::No);
    m.insert("positions", KeepField::No);

    let mut bottom = HashMap::new();
    bottom.insert("jiShen", KeepField::Yes);
    bottom.insert("taiShen", KeepField::No);
    bottom.insert("xiu", KeepField::Yes);
    bottom.insert("xiuLuck", KeepField::Yes);
    bottom.insert("zhiXing", KeepField::Yes);
    bottom.insert("liuYao", KeepField::Yes);
    bottom.insert("yueXiang", KeepField::No);
    bottom.insert("xiongSha", KeepField::Yes);
    m.insert("bottom", KeepField::Nested(bottom));

    m
}

/// Recursively filters `data` based on `schema`.
/// If schema is Yes, return data as is.
/// If schema is No, return None (discard).
/// If schema is Nested, only keep keys present in schema.
fn filter_data(data: &Value, schema: &KeepField) -> Option<Value> {
    match schema {
        KeepField::Yes => Some(data.clone()),
        KeepField::No => None,
        KeepField::Nested(sub_schema) => {
            // If data is a list, apply the schema to every item
            if let Some(arr) = data.as_array() {
                let filtered: Vec<Value> = arr
                    .iter()
                    .filter_map(|item| filter_data(item, schema))
                    .collect();
                if filtered.is_empty() {
                    None
                } else {
                    Some(Value::Array(filtered))
                }
            } else if let Some(obj) = data.as_object() {
                let mut new_obj = serde_json::Map::new();
                for (key, sub_keep) in sub_schema {
                    if let Some(val) = obj.get(*key) {
                        if let Some(filtered_val) = filter_data(val, sub_keep) {
                            new_obj.insert(key.to_string(), filtered_val);
                        }
                    }
                }
                if new_obj.is_empty() {
                    None
                } else {
                    Some(Value::Object(new_obj))
                }
            } else {
                None
            }
        }
    }
}

/// Recursively traverses to rename keys based on KEY_MAP.
fn translate_keys(data: &Value, key_map: &HashMap<&str, &str>) -> Value {
    match data {
        Value::Object(obj) => {
            let mut new_obj = serde_json::Map::new();
            for (key, value) in obj {
                let new_key = key_map
                    .get(key.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| key.clone());
                new_obj.insert(new_key, translate_keys(value, key_map));
            }
            Value::Object(new_obj)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(|v| translate_keys(v, key_map)).collect()),
        other => other.clone(),
    }
}

/// Recursively formats data into a clean string without quotes or newlines.
fn to_plaintext(data: &Value) -> String {
    match data {
        Value::Object(obj) => {
            let parts: Vec<String> = obj
                .iter()
                .map(|(key, value)| format!("{}: {}", key, to_plaintext(value)))
                .collect();
            parts.join(",\n")
        }
        Value::Array(arr) => arr
            .iter()
            .map(|item| to_plaintext(item))
            .collect::<Vec<_>>()
            .join(" "),
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
    }
}

/// 定义天干与地支的序列
/// Calculate kong wang (空亡) from day GanZhi
pub fn calculate_kong_wang(day_gan_zhi: &str) -> Result<String, String> {
    let heavenly_stems = [
        "甲", "乙", "丙", "丁", "戊", "己", "庚", "辛", "壬", "癸",
    ];
    let earthly_branches = [
        "子", "丑", "寅", "卯", "辰", "巳", "午", "未", "申", "酉", "戌", "亥",
    ];

    // Extract the first two characters (each Chinese char is multi-byte)
    let chars: Vec<char> = day_gan_zhi.chars().collect();
    if chars.len() < 2 {
        return Err("输入错误：请输入有效的天干和地支".to_string());
    }

    let gan = chars[0].to_string();
    let zhi = chars[1].to_string();

    // 1. 获取输入干支的索引 (0-9 和 0-11)
    let idx_gan = heavenly_stems
        .iter()
        .position(|&s| s == gan)
        .ok_or("输入错误：请输入有效的天干和地支")?;
    let idx_zhi = earthly_branches
        .iter()
        .position(|&s| s == zhi)
        .ok_or("输入错误：请输入有效的天干和地支")?;

    // 2. 计算"旬首" (Xun Shou) 的地支索引
    // 公式：旬首地支 = (当前地支 - 当前天干)
    // 原理：回溯到同旬的"甲"日，看它落在哪个地支上
    let xun_start_idx = (idx_zhi as isize - idx_gan as isize).rem_euclid(12) as usize;

    // 3. 计算空亡
    // 一旬有10天，从甲(0)到癸(9)。
    // 旬首(甲)对应的地支是 xun_start_idx。
    // 该旬结束时(癸)，用掉了 xun_start_idx + 9 个地支。
    // 剩下的两个地支即为空亡：(旬首 + 10) 和 (旬首 + 11)
    let kong_wang_1_idx = (xun_start_idx + 10) % 12;
    let kong_wang_2_idx = (xun_start_idx + 11) % 12;

    let kw1 = earthly_branches[kong_wang_1_idx];
    let kw2 = earthly_branches[kong_wang_2_idx];

    Ok(format!("{}{}", kw1, kw2))
}

/// Fetch almanac data from API, filter, calculate kong wang, translate, and return plaintext.
pub async fn fetch_almanac(target_date: &str) -> Result<String, String> {
    let api_url = format!(
        "https://www.mingdecode.com/api/almanac?date={}",
        target_date
    );

    let response = reqwest::get(&api_url)
        .await
        .map_err(|e| format!("Failed to retrieve data: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("Bad status {}: {}", status, "Request failed"));
    }

    let raw_data: Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse JSON: {}", e))?;

    if !raw_data.is_object() {
        return Ok(to_plaintext(&raw_data));
    }

    let keep_fields = build_keep_fields();
    let key_map = build_key_map();

    // --- STEP 1: FILTER (Keep only what we need) ---
    let filtered = filter_data(&raw_data, &KeepField::Nested(keep_fields));
    let mut filtered_data = match filtered {
        Some(v) => v,
        None => return Err("Filtered data is empty".to_string()),
    };

    // --- STEP 2: CALCULATE (Calculate kong wang) ---
    if let Some(day_gz) = filtered_data
        .get("ganZhi")
        .and_then(|gz| gz.get("day"))
        .and_then(|d| d.as_str())
    {
        match calculate_kong_wang(day_gz) {
            Ok(kw) => {
                if let Some(obj) = filtered_data.as_object_mut() {
                    obj.insert("空亡".to_string(), Value::String(kw));
                }
            }
            Err(e) => {
                error!("Kong Wang calculation error: {}", e);
            }
        }
    }

    // --- STEP 3: TRANSLATE (Rename keys to Chinese) ---
    let final_data = translate_keys(&filtered_data, &key_map);

    Ok(to_plaintext(&final_data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kong_wang() {
        // 己亥 -> 旬首 = (11-5) % 12 = 6. 空亡 = (6+10)%12=4(辰), (6+11)%12=5(巳)
        let result = calculate_kong_wang("己亥").unwrap();
        assert_eq!(result, "辰巳");
    }

    #[test]
    fn test_filter_data() {
        let raw: Value = serde_json::from_str(
            r#"{
                "solar": {"year": 2026},
                "lunar": {"monthInChinese": "腊"},
                "ganZhi": {"year": "乙巳", "month": "己丑", "day": "己亥", "time": "甲子", "timeZhi": "子"}
            }"#,
        )
        .unwrap();

        let keep = build_keep_fields();
        let filtered = filter_data(&raw, &KeepField::Nested(keep)).unwrap();

        // solar should be filtered out (KeepField::No)
        assert!(filtered.get("solar").is_none());
        // lunar should be kept
        assert!(filtered.get("lunar").is_some());
        // ganZhi should only have year, month, day
        let gz = filtered.get("ganZhi").unwrap();
        assert!(gz.get("year").is_some());
        assert!(gz.get("time").is_none());
        assert!(gz.get("timeZhi").is_none());
    }

    #[test]
    fn test_translate_keys() {
        let key_map = build_key_map();
        let data: Value = serde_json::from_str(
            r#"{"lunar": {"monthInChinese": "腊"}, "info": {"sha": "西"}}"#,
        )
        .unwrap();

        let translated = translate_keys(&data, &key_map);
        assert!(translated.get("农历").is_some());
        assert!(translated.get("基本信息").is_some());
        assert_eq!(
            translated
                .get("基本信息")
                .unwrap()
                .get("煞方")
                .unwrap()
                .as_str()
                .unwrap(),
            "西"
        );
    }

    #[test]
    fn test_to_plaintext() {
        let data: Value =
            serde_json::from_str(r#"{"key1": "val1", "key2": ["a", "b"]}"#).unwrap();
        let text = to_plaintext(&data);
        assert!(text.contains("key1: val1"));
        assert!(text.contains("key2: a b"));
    }
}
