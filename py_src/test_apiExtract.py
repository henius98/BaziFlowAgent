import json
from apiExtract import filter_data, translate_keys, KEEP_FIELDS

def test_logic():
    with open('apiSampleRespone.json', 'r', encoding='utf-8') as f:
        raw_data = json.load(f)
    
    print("Loaded sample data.")
    
    # Test Filter
    filtered_data = filter_data(raw_data, KEEP_FIELDS)
    print(f"Filtered Data keys: {list(filtered_data.keys()) if filtered_data else 'None'}")
    
    # Test Calculate Kong Wang (Validation)
    if filtered_data and 'ganZhi' in filtered_data and 'day' in filtered_data['ganZhi']:
        from apiExtract import calculate_kong_wang
        kw = calculate_kong_wang(filtered_data['ganZhi']['day'])
        print(f"Calculated Kong Wang: {kw}")
        filtered_data["空亡"] = kw

    
    # Test Translate
    final_data = translate_keys(filtered_data)
    print("Final Data:")
    print(json.dumps(final_data, ensure_ascii=False, indent=2))

if __name__ == "__main__":
    test_logic()
