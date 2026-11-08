use super::*;

#[test]
fn wide_picture_shrinks_to_the_limit() {
    assert_eq!(fit_to_side(16000, 4000, 8192), (8192, 2048));
    assert_eq!(fit_to_side(4000, 16000, 8192), (2048, 8192));
}

#[test]
fn small_picture_is_untouched() {
    assert_eq!(fit_to_side(800, 600, 8192), (800, 600));
    assert_eq!(fit_to_side(8192, 8192, 8192), (8192, 8192));
}

#[test]
fn thin_picture_keeps_at_least_one_pixel() {
    assert_eq!(fit_to_side(16384, 1, 8192), (8192, 1));
}

#[test]
fn eviction_drops_oldest_first() {
    let mut inner = Inner {
        entries: HashMap::new(),
        lru: VecDeque::new(),
        in_flight: HashSet::new(),
    };
    for i in 0..(MAX_ENTRIES + 10) {
        insert(&mut inner, format!("u{i}"), Entry::Failed("x".into()));
    }
    assert_eq!(inner.entries.len(), MAX_ENTRIES);
    assert!(!inner.entries.contains_key("u0"));
    assert!(!inner.entries.contains_key("u9"));
    assert!(inner.entries.contains_key(&format!("u{}", MAX_ENTRIES + 9)));
    // The lru list and the map stay the same length: no ghost keys.
    assert_eq!(inner.lru.len(), inner.entries.len());
}

#[test]
fn refresh_of_known_url_does_not_duplicate_lru() {
    let mut inner = Inner {
        entries: HashMap::new(),
        lru: VecDeque::new(),
        in_flight: HashSet::new(),
    };
    insert(&mut inner, "u".into(), Entry::Failed("a".into()));
    insert(&mut inner, "u".into(), Entry::Failed("b".into()));
    assert_eq!(inner.lru.len(), 1);
    assert!(matches!(inner.entries["u"], Entry::Failed(ref e) if e == "b"));
}
