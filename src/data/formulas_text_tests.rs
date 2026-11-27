use super::*;

#[test]
fn search_cache_keeps_recent_regexes_and_caps_the_number_of_patterns() {
    let mut patterns = SearchPatterns(VecDeque::new());
    for i in 0..SEARCH_CACHE_SLOTS {
        assert_eq!(
            patterns.find(&format!("SKU-{i}"), &format!("pre-sku-{i}")),
            Some(4)
        );
    }

    let oldest_pattern = "SKU-0";
    let cached_key = patterns.0.back().unwrap().0.as_ptr();
    assert_eq!(
        patterns.find(oldest_pattern, "SKU-0 followed by sku-0"),
        Some(0)
    );
    assert_eq!(patterns.0.front().unwrap().0.as_ptr(), cached_key);

    assert_eq!(patterns.find("new?", "pré-NEW1"), Some(5));
    assert_eq!(patterns.0.len(), SEARCH_CACHE_SLOTS);
    assert!(!patterns.0.iter().any(|(key, _)| key.as_ref() == "SKU-1"));
    assert_eq!(patterns.find(oldest_pattern, "nonmatching"), None);
    assert_eq!(patterns.0.front().unwrap().0.as_ptr(), cached_key);
}
