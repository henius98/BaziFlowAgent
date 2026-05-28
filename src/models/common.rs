/// Represents a city and its geographical data.
#[derive(Debug, Clone)]
pub struct City {
    pub name: &'static str,
    pub longitude: f64,
}

/// A list of common cities for selection.
pub const COMMON_CITIES: &[City] = &[
    City {
        name: "吉隆坡 (Kuala Lumpur)",
        longitude: 101.68,
    },
    City {
        name: "马六甲 (Malacca)",
        longitude: 102.25,
    },
    City {
        name: "新加坡 (Singapore)",
        longitude: 103.85,
    },
    City {
        name: "北京 (Beijing)",
        longitude: 116.40,
    },
    City {
        name: "上海 (Shanghai)",
        longitude: 121.47,
    },
    City {
        name: "广州 (Guangzhou)",
        longitude: 113.26,
    },
    City {
        name: "深圳 (Shenzhen)",
        longitude: 114.05,
    },
    City {
        name: "香港 (Hong Kong)",
        longitude: 114.17,
    },
    City {
        name: "台北 (Taipei)",
        longitude: 121.56,
    },
];

// Bazi records
pub const STEMS: [&str; 10] = ["甲", "乙", "丙", "丁", "戊", "己", "庚", "辛", "壬", "癸"];
pub const BRANCHES: [&str; 12] = ["子", "丑", "寅", "卯", "辰", "巳", "午", "未", "申", "酉", "戌", "亥"];
pub const STATES: [&str; 12] = ["长生", "沐浴", "冠带", "临官", "帝旺", "衰", "病", "死", "墓", "绝", "胎", "养"];

// Calender
pub const MONTH_NAME: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
pub const DAY_HEADERS: [&str; 7] = ["Mon", "Tue", "Wed", "Thur", "Fri", "Sat", "Sun"];
