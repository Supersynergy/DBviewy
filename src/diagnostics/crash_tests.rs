use super::*;

#[test]
fn detects_unclean_then_marks() {
    let dir = tempfile::tempdir().unwrap();
    let (unclean, first) = mark_running_in(dir.path());
    assert!(!unclean);
    let first = first.unwrap();
    let (unclean, second) = mark_running_in(dir.path());
    assert!(!unclean, "live session must not trigger recovery");
    drop(second.unwrap());
    assert!(first.path.exists());
    std::fs::write(dir.path().join("session-abandoned.lock"), []).unwrap();
    let (unclean, third) = mark_running_in(dir.path());
    assert!(unclean);
    drop(third.unwrap());
    drop(first);
    assert_eq!(dir.path().read_dir().unwrap().count(), 0);
}

#[test]
fn panic_message_handles_str_and_string() {
    let s: &str = "boom";
    assert_eq!(panic_message(&s), "boom");
    let owned: String = "kaboom".to_string();
    assert_eq!(panic_message(&owned), "kaboom");
}

#[cfg(unix)]
#[test]
fn a_live_legacy_session_is_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let legacy = dir.path().join("running.lock");
    std::fs::write(&legacy, format!("pid {}", std::process::id())).unwrap();
    let (unclean, marker) = mark_running_in(dir.path());
    assert!(!unclean);
    drop(marker.unwrap());
    assert!(legacy.exists());
}
