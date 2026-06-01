use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct StructuredBazi {
    #[serde(rename = "基本信息")]
    pub info: BasicInfo,
    #[serde(rename = "四柱排盘")]
    pub pillars: Vec<PillarData>,
    #[serde(rename = "其他")]
    pub other: AdditionalInfo,
    #[serde(flatten)] // 留意关系
    pub relation: Option<Relations>,
    #[serde(rename = "用事", skip_serializing_if = "Option::is_none")]
    pub yongshi: Option<String>,
    #[serde(rename = "五行旺衰")]
    pub element_states: ElementStates,
    #[serde(rename = "起运信息")]
    pub luck_info: LuckInfo,
    #[serde(rename = "大运")]
    pub dayun: Vec<DaYunData>,
    #[serde(rename = "流年", skip_serializing_if = "Option::is_none")]
    pub liunian: Option<LiuNian>,
    #[serde(default, rename = "原局整柱", skip_serializing_if = "Vec::is_empty")]
    pub pillar_traits: Vec<String>, // 盖头, 截脚, 伏吟, 反吟
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BasicInfo {
    #[serde(rename = "性别")] // 乾造, 坤造
    pub gender: String,
    #[serde(rename = "农历")]
    pub lunisolar_date: String,
    #[serde(rename = "阳历")]
    pub solar_date: String,
    #[serde(rename = "出生地")]
    pub birth_location: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PillarData {
    #[serde(rename = "柱名")]
    pub name: String,
    #[serde(flatten)]
    pub base: BasePillarData,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AdditionalInfo {
    #[serde(flatten)] // 三垣
    pub palace_info: PalaceInfo,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LuckInfo {
    #[serde(rename = "起运岁")]
    pub start_age: String,
    #[serde(rename = "交运时间")]
    pub transition_time: String,
    #[serde(rename = "起运时间表")]
    pub start_ages: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DaYunData {
    #[serde(rename = "当前大运")]
    pub is_current_dayun: bool,
    #[serde(rename = "起始年份")]
    pub start_year: i32,
    #[serde(rename = "结束年份")]
    pub end_year: i32,
    #[serde(flatten)]
    pub info: BasePillarData,
    #[serde(flatten)]
    pub relation: Option<Relations>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LiuNian {
    #[serde(rename = "流年")]
    pub year: i32,
    #[serde(flatten)]
    pub info: BasePillarData,
    #[serde(flatten)]
    pub relation: Option<Relations>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ElementStates {
    #[serde(rename = "木")]
    pub wood: String,
    #[serde(rename = "火")]
    pub fire: String,
    #[serde(rename = "土")]
    pub earth: String,
    #[serde(rename = "金")]
    pub metal: String,
    #[serde(rename = "水")]
    pub water: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Relations {
    #[serde(rename = "天干关系", skip_serializing_if = "Option::is_none")]
    pub stem_relations: Option<Vec<String>>,
    #[serde(rename = "地支关系", skip_serializing_if = "Option::is_none")]
    pub branch_relations: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BasePillarData {
    // #[serde(rename = "主星")]
    // pub main_star: String,
    #[serde(rename = "天干&主星")]
    pub stem_and_stars: Vec<(String, String)>,
    #[serde(rename = "地支")]
    pub branch: String,
    #[serde(rename = "藏干&副星")]
    pub hidden_stems_and_stars: Vec<(String, String)>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PalaceInfo {
    // #[serde(alias = "taixi", rename = "胎息")]
    // pub fetal_breath: String,
    // #[serde(alias = "taixi_nayin", rename = "胎息纳音")]
    // pub fetal_breath_nayin: String,
    #[serde(alias = "taiyuan", rename = "胎元")]
    pub fetal_origin: String,
    #[serde(alias = "taiyuan_nayin", rename = "胎元纳音")]
    pub fetal_origin_nayin: String,
    #[serde(alias = "minggong", rename = "命宫")]
    pub life_palace: String,
    #[serde(alias = "minggong_nayin", rename = "命宫纳音")]
    pub life_palace_nayin: String,
    #[serde(alias = "shenggong", rename = "身宫")]
    pub body_palace: String,
    #[serde(alias = "shenggong_nayin", rename = "身宫纳音")]
    pub body_palace_nayin: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RawBaziChart {
    #[serde(alias = "bz", rename = "八字")]
    pub bz: Bazi,
    #[serde(alias = "ss", rename = "天干十神")]
    pub ss: Vec<String>,
    #[serde(alias = "cg", rename = "藏干")]
    pub cg: Vec<Vec<String>>,
    #[serde(alias = "cgss", rename = "藏干十神")]
    pub cgss: Vec<Vec<String>>,

    #[serde(alias = "xy", rename = "星运")]
    pub xy: Vec<String>,
    #[serde(alias = "zz", rename = "自坐")]
    pub zz: Vec<String>,
    #[serde(alias = "kw", rename = "空亡柱")]
    pub kw: Vec<String>,
    #[serde(alias = "ny", rename = "纳音")]
    pub ny: Vec<String>,
    #[serde(alias = "szshensha", rename = "四柱神煞")]
    pub szshensha: Vec<Vec<String>>,

    #[serde(flatten)] // 三垣
    pub palace_info: PalaceInfo,

    #[serde(alias = "qiyunsui", rename = "起运岁")]
    pub qiyunsui: u8,
    #[serde(alias = "qiyunarr", rename = "起运时间表")]
    pub qiyunarr: [u8; 6],
    #[serde(alias = "jiaoyun", rename = "交运时间")]
    pub jiaoyun: String,
    #[serde(alias = "kongwang", rename = "空亡")]
    pub kongwang: String,
    // #[serde(alias = "dayun", rename = "大运")]   // this duplicate value within dyshensha, only keep dyshensha enough
    // pub dayun: Vec<String>,
    #[serde(alias = "dyshensha", rename = "大运神煞")]
    pub dyshensha: Vec<(String, Vec<String>)>,
    // #[serde(alias = "xiaoyun", rename = "小运")]
    // pub xiaoyun: Vec<String>,

    // Extra
    #[serde(rename = "用事", skip_serializing_if = "Option::is_none")]
    pub yongshi: Option<String>,
    #[serde(skip, rename = "原局干支关系")]
    pub ori_gz_relations: Option<Vec<Vec<String>>>,
    #[serde(skip, rename = "大运干支关系")]
    pub dy_gz_relations: Option<Vec<Vec<String>>>,
    #[serde(skip, rename = "流年干支关系")]
    pub ln_gz_relations: Option<Vec<Vec<String>>>,
    #[serde(skip, rename = "流年神煞")]
    pub ln_shensha: Option<Vec<String>>,
    // #[serde(skip)]
    // pub ln_info: Option<BasePillarData>,
    // #[serde(skip)]
    // pub dy_info: Option<BasePillarData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bazi {
    #[serde(alias = "0", rename = "year_steam")]
    pub year_steam: String,
    #[serde(alias = "1", rename = "year_branch")]
    pub year_branch: String,
    #[serde(alias = "2", rename = "month_steam")]
    pub month_steam: String,
    #[serde(alias = "3", rename = "month_branch")]
    pub month_branch: String,
    #[serde(alias = "4", rename = "day_steam")]
    pub day_steam: String,
    #[serde(alias = "5", rename = "day_branch")]
    pub day_branch: String,
    #[serde(alias = "6", rename = "hour_steam")]
    pub hour_steam: String,
    #[serde(alias = "7", rename = "hour_branch")]
    pub hour_branch: String,
    #[serde(default, alias = "8", rename = "农历")]
    pub lunisolar_date: String,
}
