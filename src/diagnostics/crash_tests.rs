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

/// A panic inside `caught` is an error the caller handles, and the hook
/// sees it as caught, so no crash report waits at the next start. Outside,
/// and after it, a panic is a crash again.
#[test]
fn a_caught_panic_is_an_error_and_no_crash() {
    assert!(!panic_is_caught());
    assert_eq!(caught(|| 7), Ok(7));
    assert_eq!(caught(panic_is_caught), Ok(true));
    assert_eq!(
        caught(|| -> () { panic!("bad buffer") }),
        Err("bad buffer".to_string())
    );
    assert!(!panic_is_caught(), "the flag outlived the panic");
    // Nested: the inner one leaves the outer one caught.
    assert_eq!(
        caught(|| {
            let _ = caught(|| -> () { panic!("inner") });
            panic_is_caught()
        }),
        Ok(true)
    );
}
