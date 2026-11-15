use super::*;

fn draw_with_result(result: Result<String>) -> (String, bool, Option<String>) {
    let (tx, rx) = mpsc::channel();
    tx.send(result).unwrap();
    let mut voice = VoiceInput {
        result: Some(rx),
        ..Default::default()
    };
    let mut draft = String::from("Check:");
    let mut focus = false;
    let ctx = egui::Context::default();
    let mut frame = ctx.run_ui(Default::default(), |ui| {
        voice.show(ui, &mut draft, &mut focus)
    });
    frame.textures_delta.clear();
    (draft, focus, voice.error)
}

#[test]
fn voice_result_appends_to_the_editable_draft() {
    let (draft, focus, error) = draw_with_result(Ok("show sales".into()));
    assert_eq!(draft, "Check: show sales");
    assert!(focus);
    assert!(error.is_none());
}

#[test]
fn failed_recognition_keeps_the_existing_draft() {
    let (draft, focus, error) = draw_with_result(Err(anyhow::anyhow!("No speech recognised")));
    assert_eq!(draft, "Check:");
    assert!(!focus);
    assert_eq!(error.as_deref(), Some("No speech recognised"));
}

/// The fixture and installed GGUF runner are supplied by the native acceptance run.
#[test]
#[ignore = "requires a local Parakeet Ultra installation and synthetic audio fixture"]
fn ultra_audio_reaches_the_editable_draft() {
    let audio = std::env::var_os("OCTA_VOICE_TEST_AUDIO").expect("synthetic speech fixture");
    let expected = std::env::var("OCTA_VOICE_TEST_TEXT").expect("expected transcription");
    let work = tempfile::tempdir().unwrap();
    let (_guard, cancel) = mpsc::channel();
    let result = transcribe(Path::new(&audio), work.path(), &cancel);
    let (draft, focus, error) = draw_with_result(result);
    assert!(error.is_none(), "{error:?}");
    assert_eq!(draft, format!("Check: {expected}"));
    assert!(focus);
    println!("{draft}");
}

#[cfg(unix)]
#[test]
fn dropping_voice_input_reaps_child_and_cleans_audio() {
    use std::sync::mpsc::channel;
    let (guard, cancel) = channel();
    let panel = VoiceInput {
        cancel: Some(guard),
        ..Default::default()
    };
    let audio = tempfile::tempdir().unwrap();
    let directory = audio.path().to_owned();
    let pid_file = directory.join("pid");
    let worker = std::thread::spawn(move || {
        let mut cmd = Command::new("/bin/sh");
        cmd.arg("-c")
            .arg("echo $$ > pid; exec /bin/sleep 30")
            .current_dir(audio.path());
        run_owned(&mut cmd, &cancel, Duration::from_secs(10), None)
    });
    let deadline = Instant::now() + Duration::from_secs(4);
    let pid = loop {
        if let Ok(s) = std::fs::read_to_string(&pid_file)
            && let Ok(pid) = s.trim().parse::<i32>()
        {
            break pid;
        }
        assert!(Instant::now() < deadline, "child did not start");
        std::thread::sleep(Duration::from_millis(10));
    };
    drop(panel);
    let started = Instant::now();
    assert!(
        worker
            .join()
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("cancelled")
    );
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(!directory.exists());
    // Signal 0 only tests whether the specific child still exists.
    assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ESRCH)
    );
}

#[cfg(unix)]
#[test]
fn timed_out_voice_process_is_reaped() {
    let (_guard, cancel) = mpsc::channel();
    let mut cmd = Command::new("/bin/sleep");
    cmd.arg("30");
    let started = Instant::now();
    let err = run_owned(&mut cmd, &cancel, Duration::from_millis(80), None).unwrap_err();
    assert!(err.to_string().contains("timed out"));
    assert!(started.elapsed() < Duration::from_secs(3));
}
