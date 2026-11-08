use super::*;

fn cache_at(path: std::path::PathBuf) -> FetchCache {
    FetchCache::load(path)
}

#[test]
fn get_returns_what_put_stored() {
    let dir = std::env::temp_dir().join(format!("octa-fc-{}", std::process::id()));
    let c = cache_at(dir.join("a.json"));
    c.put(
        "http://x/1",
        "http://x/final",
        200,
        "text/html",
        "<p>hi</p>",
    );
    let (u, s, ct, b, _) = c.get_stamped("http://x/1").expect("fresh hit");
    assert_eq!(
        (u.as_str(), s, ct.as_str(), b.as_str()),
        ("http://x/final", 200, "text/html", "<p>hi</p>")
    );
    assert!(c.get_stamped("http://x/missing").is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn stale_entries_miss() {
    let dir = std::env::temp_dir().join(format!("octa-fc2-{}", std::process::id()));
    let c = cache_at(dir.join("b.json"));
    c.put("http://x/1", "http://x/1", 200, "t", "b");
    {
        let mut map = c.map.lock().unwrap();
        map.get_mut("http://x/1").unwrap().fetched_at -= TTL_SECS + 1;
    }
    assert!(c.get_stamped("http://x/1").is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn oversized_bodies_are_not_cached() {
    let c = cache_at(std::env::temp_dir().join("octa-fc3.json"));
    let big = "x".repeat(MAX_BODY_BYTES + 1);
    c.put("http://x/big", "http://x/big", 200, "t", &big);
    assert!(c.get_stamped("http://x/big").is_none());
}

#[test]
fn oldest_entries_are_pruned() {
    let c = cache_at(std::env::temp_dir().join("octa-fc4.json"));
    for i in 0..(MAX_ENTRIES + 5) {
        let url = format!("http://x/{i}");
        c.put(&url, &url, 200, "t", "b");
        // Age entries monotonically so pruning order is deterministic.
        c.map.lock().unwrap().get_mut(&url).unwrap().fetched_at = i as i64;
    }
    let len = c.map.lock().unwrap().len();
    assert!(len <= MAX_ENTRIES);
    // The five oldest must be gone; the newest survives.
    assert!(c.get_stamped("http://x/0").is_none());
    assert!(c.get_stamped("http://x/4").is_none());
}

#[test]
fn flush_persists_and_reload_hits() {
    let dir = std::env::temp_dir().join(format!("octa-fc5-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("c.json");
    {
        let c = cache_at(path.clone());
        c.put("http://x/1", "http://x/1", 200, "text/html", "<b>v</b>");
        c.flush();
    }
    let c2 = cache_at(path);
    assert!(c2.get_stamped("http://x/1").is_some(), "reload should hit");
    let _ = std::fs::remove_dir_all(&dir);
}
