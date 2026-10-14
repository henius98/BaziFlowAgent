/// Represents a city and its geographical data.
#[derive(Debug, Clone)]
pub struct City {
  pub name: &'static str,
  pub longitude: f64,
}

/// A list of common cities for selection.
pub const COMMON_CITIES: &[City] = &[
  City { name: "吉隆坡 (Kuala Lumpur)", longitude: 101.68 },
  City { name: "文冬 (Bentong)", longitude: 101.91 },
  City { name: "马六甲 (Malacca)", longitude: 102.25 },
  City { name: "新加坡 (Singapore)", longitude: 103.85 },
  City { name: "北京 (Beijing)", longitude: 116.40 },
  City { name: "上海 (Shanghai)", longitude: 121.47 },
  City { name: "广州 (Guangzhou)", longitude: 113.26 },
  City { name: "深圳 (Shenzhen)", longitude: 114.05 },
  City { name: "香港 (Hong Kong)", longitude: 114.17 },
  City { name: "台北 (Taipei)", longitude: 121.56 },
];

// Bazi records
pub const STEMS: [&str; 10] = ["甲", "乙", "丙", "丁", "戊", "己", "庚", "辛", "壬", "癸"];
pub const BRANCHES: [&str; 12] = ["子", "丑", "寅", "卯", "辰", "巳", "午", "未", "申", "酉", "戌", "亥"];
pub const STATES: [&str; 12] = ["长生", "沐浴", "冠带", "临官", "帝旺", "衰", "病", "死", "墓", "绝", "胎", "养"];

/// Five Element (五行) type for the overcoming/destroying (克) cycle
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum WuXing {
  Wood,  // 木
  Fire,  // 火
  Earth, // 土
  Metal, // 金
  Water, // 水
}

impl WuXing {
  /// Overcoming/Destroying cycle (克): Wood→Earth→Water→Fire→Metal→Wood
  pub fn destroys(self, target: WuXing) -> bool {
    matches!((self, target), (Self::Wood, Self::Earth) | (Self::Earth, Self::Water) | (Self::Water, Self::Fire) | (Self::Fire, Self::Metal) | (Self::Metal, Self::Wood))
  }
  /// Generating cycle (生): Wood→Fire→Earth→Metal→Water→Wood
  pub fn generates(self, target: WuXing) -> bool {
    matches!((self, target), (Self::Wood, Self::Fire) | (Self::Fire, Self::Earth) | (Self::Earth, Self::Metal) | (Self::Metal, Self::Water) | (Self::Water, Self::Wood))
  }
}

// Calendar
pub const MONTH_NAME: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
pub const DAY_HEADERS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_wuxing_generating_cycle() {
    assert!(WuXing::Wood.generates(WuXing::Fire));
    assert!(WuXing::Fire.generates(WuXing::Earth));
    assert!(WuXing::Earth.generates(WuXing::Metal));
    assert!(WuXing::Metal.generates(WuXing::Water));
    assert!(WuXing::Water.generates(WuXing::Wood));

    assert!(!WuXing::Wood.generates(WuXing::Earth));
  }

  #[test]
  fn test_wuxing_destroying_cycle() {
    assert!(WuXing::Wood.destroys(WuXing::Earth));
    assert!(WuXing::Earth.destroys(WuXing::Water));
    assert!(WuXing::Water.destroys(WuXing::Fire));
    assert!(WuXing::Fire.destroys(WuXing::Metal));
    assert!(WuXing::Metal.destroys(WuXing::Wood));

    assert!(!WuXing::Wood.destroys(WuXing::Fire));
  }
}
