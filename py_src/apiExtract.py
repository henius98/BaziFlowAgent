import datetime
import requests
import json

# 1. MAPPING DICTIONARY (English -> Chinese)
KEY_MAP = {
    # Sections
    "data": "data",
    "bottom": "额外补充",
    "ganZhi": "干支",
    "info": "基本信息",
    "lunar": "农历",
    "solar": "公历",
    "yiJi": "宜忌",
    "positions": "吉神方位",
    "zodiac": "生肖",
    "hours": "时辰吉凶",

    # Specific Fields - YiJi (Suit/Avoid)
    "yi": "宜",
    "ji": "忌",
    
    # Specific Fields - Shen (Gods/Luck)
    "jiShen": "吉神宜趋",
    "xiongSha": "凶煞宜忌",
    "tianShen": "值神",
    "taiShen": "今日胎神",
    
    # Specific Fields - Astrology/Physics
    "liuYao": "六曜",
    "xiu": "二十八星宿",
    "xiuLuck": "星宿吉凶",
    "yueXiang": "月相",
    "zhiXing": "建除十二神", # Or 十二建星
    "xingZuo": "星座",
    
    # Specific Fields - Positions
    "cai": "财神",
    "xi": "喜神",
    "fu": "福神",
    "yangGui": "阳贵神",
    "yinGui": "阴贵神",
    "dayTai": "逐日胎神",
    "monthTai": "逐月胎神",
    "yearTai": "逐年胎神",

    # Specific Fields - Clash/Details
    "chongDesc": "冲煞",
    "chongShengXiao": "冲生肖",
    "sha": "煞方",
    "luck": "吉凶",
    
    # Dates & Time
    "year": "年",
    "month": "月",
    "day": "日",
    "time": "时",
    "weekInChinese": "星期",
    "dayInChinese": "农历日",
    "monthInChinese": "农历月",
    
    # NaYin (Sound)
    "yearNaYin": "年纳音",
    "monthNaYin": "月纳音",
    "dayNaYin": "日纳音",
    
    # Pillars
    "timeZhi": "时支",
    "zhi": "地支"
}

# 2. FILTER CONFIGURATION (Whitelist)
# Only keys defined here will be kept.
KEEP_FIELDS = {
    "solar": False,
    "lunar": True,
    "ganZhi": {
        "year": True,
        "month": True,
        "day": True,
        "time": False,
        "timeZhi": False
    },
    "zodiac": False,
    "yiJi": False,
    "info": True,
    "hours": False,
    "positions": False,
    "bottom": {
        "jiShen": True,
        "taiShen": False,
        "xiu": True,
        "xiuLuck": True,
        "zhiXing": True,
        "liuYao": True,
        "yueXiang": False,
        "xiongSha": True
    }
}

def filter_data(data, schema):
    """
    Recursively filters 'data' based on 'schema'.
    If schema is True, return data as is.
    If schema is False, return None (discard).
    If schema is a dict, only keep keys present in schema.
    """
    if schema is True:
        return data
    
    if schema is False:
        return None
    
    # If data is a list (like 'hours'), apply the schema to every item in the list
    if isinstance(data, list):
        # Apply filter to each item
        filtered_list = [filter_data(item, schema) for item in data]
        # Remove None items (where schema rejected the item)
        filtered_list = [item for item in filtered_list if item is not None]
        # If the list is empty or we want to return None if all items are filtered out? 
        # Usually if the schema was a dict, we want the filtered list.
        # If schema was False, we already returned None above.
        return filtered_list

    # If data is a dict, recurse deeper
    if isinstance(data, dict) and isinstance(schema, dict):
        new_data = {}
        for key, sub_schema in schema.items():
            if key in data:
                filtered_val = filter_data(data[key], sub_schema)
                # We only add the key if the value is not None. 
                # Note: valid empty headers/lists are not None, so they are kept.
                if filtered_val is not None:
                    new_data[key] = filtered_val
        return new_data
        
    return None

def translate_keys(data):
    """Recursively traverses to rename keys based on KEY_MAP."""
    if isinstance(data, dict):
        new_data = {}
        for key, value in data.items():
            new_key = KEY_MAP.get(key, key)
            new_data[new_key] = translate_keys(value)
        return new_data
    elif isinstance(data, list):
        return [translate_keys(item) for item in data]
    else:
        return data

def to_plaintext(data):
    """Recursively formats data into a clean string without quotes or newlines."""
    if isinstance(data, dict):
        parts = []
        for key, value in data.items():
            parts.append(f"{key}: {to_plaintext(value)}")
        return ",\n".join(parts)
    elif isinstance(data, list):
        return " ".join([to_plaintext(item) for item in data])
    else:
        return str(data)

def calculate_kong_wang(dayGanZhi):
    # 定义天干与地支的序列
    heavenly_stems = ["甲", "乙", "丙", "丁", "戊", "己", "庚", "辛", "壬", "癸"]
    earthly_branches = ["子", "丑", "寅", "卯", "辰", "巳", "午", "未", "申", "酉", "戌", "亥"]
    
    # 1. 获取输入干支的索引 (0-9 和 0-11)
    if dayGanZhi[0] not in heavenly_stems or dayGanZhi[1] not in earthly_branches:
        return "输入错误：请输入有效的天干和地支"
        
    idx_gan = heavenly_stems.index(dayGanZhi[0])
    idx_zhi = earthly_branches.index(dayGanZhi[1])
    
    # 2. 计算“旬首” (Xun Shou) 的地支索引
    # 公式：旬首地支 = (当前地支 - 当前天干) 
    # 原理：回溯到同旬的“甲”日，看它落在哪个地支上
    xun_start_idx = (idx_zhi - idx_gan) % 12
    
    # 3. 计算空亡
    # 一旬有10天，从甲(0)到癸(9)。
    # 旬首(甲)对应的地支是 xun_start_idx。
    # 该旬结束时(癸)，用掉了 xun_start_idx + 9 个地支。
    # 剩下的两个地支即为空亡：(旬首 + 10) 和 (旬首 + 11)
    kong_wang_1_idx = (xun_start_idx + 10) % 12
    kong_wang_2_idx = (xun_start_idx + 11) % 12
    
    kw1 = earthly_branches[kong_wang_1_idx]
    kw2 = earthly_branches[kong_wang_2_idx]
    
    return kw1+kw2

def main(target_date: str) -> dict:
    try:
        api_url = f"https://www.mingdecode.com/api/almanac?date={target_date}"
        response = requests.get(api_url, timeout=10)
        response.raise_for_status()
        raw_data = response.json()
        
        if isinstance(raw_data, dict):
            # --- STEP 1: FILTER (Keep only what we need) ---
            filtered_data = filter_data(raw_data, KEEP_FIELDS)

            # --- STEP 2: CALCULATE (Calculate kong wang) ---
            result = calculate_kong_wang(filtered_data["ganZhi"]["day"])
            filtered_data["空亡"] = result

            # --- STEP 3: TRANSLATE (Rename keys to Chinese) ---
            final_data = translate_keys(filtered_data)
        else:
            final_data = raw_data

        return {
            "status": response.status_code,
            "data": to_plaintext(final_data)
        }
        
    except Exception as e:
        return {
            "status": 400,
            "data": {
                "error": "Failed to retrieve data", 
                "details": str(e)
            }
        }

if __name__ == "__main__":
    # Test with tomorrow's date or a fixed date
    test_date = (datetime.date.today()).strftime("%Y-%m-%d")
    print(f"Testing with date: {test_date}")
    result = main(test_date)
    print(json.dumps(result, ensure_ascii=False, indent=2))