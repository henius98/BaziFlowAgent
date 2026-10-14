// 留意关系
// 盖头, 截脚, 伏吟, 反吟
// 乾造, 坤造
// Profiles saved before this field existed always required an exact time.
// 三垣
// #[serde(rename = "主星")]
// pub main_star: String,
// #[serde(alias = "taixi", rename = "胎息")]
// pub fetal_breath: String,
// #[serde(alias = "taixi_nayin", rename = "胎息纳音")]
// pub fetal_breath_nayin: String,
// #[serde(alias = "dayun", rename = "大运")]   // this duplicate value within dyshensha, only keep dyshensha enough
// pub dayun: Vec<String>,
// #[serde(alias = "xiaoyun", rename = "小运")]
// pub xiaoyun: Vec<String>,
// Extra
// #[serde(skip)]
// pub ln_info: Option<BasePillarData>,
// #[serde(skip)]
// pub dy_info: Option<BasePillarData>,
pub use baziflow_core::models::*;

#[cfg(test)]
mod tests {
  use super::BasicInfo;

  #[test]
  fn legacy_profile_defaults_birth_time_to_known() {
    let legacy = r#"{
            "性别": "男,乾造",
            "农历": "一九八九年十二月初五",
            "阳历": "1990-01-01 00:00",
            "出生地": "未知"
        }"#;
    let info: BasicInfo = serde_json::from_str(legacy).expect("legacy profile must deserialize");

    assert!(info.birth_time_known);
  }
}
