//! Offline speech input. Only the editable draft receives the recognition result.
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use crate::ui::settings::AppSettings;
use anyhow::{Context, Result, bail};
use eframe::egui;
use octa::i18n::t;

#[derive(Default)]
pub(super) struct VoiceInput {
    pub(super) completed_command: bool,
    source: Option<String>,
    stop: Option<Sender<()>>,
    // Dropping the active panel/session disconnects the worker in every stage.
    cancel: Option<Sender<()>>,
    result: Option<Receiver<Result<String>>>,
    error: Option<String>,
}

// Finder opens apps with a shorter PATH than the interactive shell.
fn local_command(program: &str) -> Result<Command> {
    let mut paths: Vec<_> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    if let Some(home) = std::env::var_os("HOME") {
        paths.push(PathBuf::from(home).join(".local/bin"));
    }
    paths.extend(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
    let mut cmd = Command::new(program);
    cmd.env("PATH", std::env::join_paths(paths)?);
    Ok(cmd)
}

fn run_owned(
    cmd: &mut Command,
    cancel: &Receiver<()>,
    timeout: Duration,
    finish_recording: Option<&Receiver<()>>,
) -> Result<ExitStatus> {
    let cancelled = || !matches!(cancel.try_recv(), Err(mpsc::TryRecvError::Empty));
    if cancelled() {
        bail!("Voice input cancelled");
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd.spawn()?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {}
            Err(error) => {
                crate::app::chat::ollama::stop_child_group(&mut child);
                return Err(error.into());
            }
        }
        let closed = cancelled();
        if closed || Instant::now() >= deadline {
            crate::app::chat::ollama::stop_child_group(&mut child);
            if closed {
                bail!("Voice input cancelled");
            }
            bail!("Voice input timed out");
        }
        if finish_recording.is_some_and(|stop| stop.try_recv().is_ok())
            && let Some(mut stdin) = child.stdin.take()
        {
            let _ = stdin.write_all(b"q\n");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn record(wav: &Path, stop: Receiver<()>, cancel: &Receiver<()>) -> Result<()> {
    let mut recorder = local_command("ffmpeg")?;
    recorder
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "avfoundation",
            "-i",
            ":default",
            "-t",
            "60",
            "-ar",
            "16000",
            "-ac",
            "1",
        ])
        .arg(wav)
        .stdin(Stdio::piped())
        .stdout(Stdio::null());
    let status = run_owned(&mut recorder, cancel, Duration::from_secs(70), Some(&stop))
        .context("Record the default microphone with ffmpeg")?;
    if !status.success() {
        bail!("Microphone recording failed ({status}). Check microphone access for Octa.");
    }
    Ok(())
}

fn transcribe(audio: &Path, directory: &Path, cancel: &Receiver<()>) -> Result<String> {
    let mlx = std::env::var_os("OCTA_VOICE_BACKEND").as_deref() == Some("mlx".as_ref());
    let transcript = if mlx {
        transcribe_mlx(audio, directory, cancel)?
    } else {
        let config =
            AppSettings::config_dir().context("Locate the Octa configuration directory")?;
        let model = config.join("parakeet-ultra-Q8_0.gguf");
        let runner = config.join("parakeet-ultra");
        if !model.is_file() || !runner.is_file() {
            bail!("Set up the local Parakeet Ultra Q8_0 model and runner. See Help > Voice input.");
        }
        let pcm = directory.join("audio.f32le");
        let mut decode = local_command("ffmpeg")?;
        decode
            .args(["-nostdin", "-loglevel", "error", "-i"])
            .arg(audio)
            .args(["-vn", "-ar", "16000", "-ac", "1", "-f", "f32le"])
            .arg(&pcm);
        let status = run_owned(&mut decode, cancel, Duration::from_secs(180), None)
            .context("Decode the audio with ffmpeg")?;
        if !status.success() {
            bail!("Audio decoding failed ({status})");
        }
        // Files leave the child free to write while the worker watches cancellation.
        let transcript = directory.join("transcript.txt");
        let diagnostic = directory.join("diagnostic.txt");
        let mut command = Command::new(runner);
        command
            .arg(model)
            .arg(pcm)
            .stdin(Stdio::null())
            .stdout(std::fs::File::create(&transcript)?)
            .stderr(std::fs::File::create(&diagnostic)?);
        let status = run_owned(&mut command, cancel, Duration::from_secs(600), None)
            .context("Run local Parakeet Ultra")?;
        if !status.success() {
            let diagnostics = std::fs::read_to_string(diagnostic).unwrap_or_default();
            let detail = diagnostics.lines().last().unwrap_or("No diagnostic");
            bail!("Parakeet Ultra failed ({status}): {detail}");
        }
        std::fs::read_to_string(transcript).context("Read the Parakeet Ultra transcript")?
    };
    let transcript = transcript.trim();
    if transcript.is_empty() {
        bail!("No speech recognised");
    }
    Ok(transcript.to_owned())
}

fn transcribe_mlx(audio: &Path, directory: &Path, cancel: &Receiver<()>) -> Result<String> {
    let mut cmd = local_command("parakeet-mlx")?;
    cmd.env("HF_HUB_OFFLINE", "1")
        .env("HF_HUB_DISABLE_TELEMETRY", "1")
        .args([
            "--output-format",
            "txt",
            "--output-template",
            "transcript",
            "--output-dir",
        ])
        .arg(directory);
    if std::env::var_os("PARAKEET_MODEL").is_none()
        && let Some(model) = AppSettings::config_dir().map(|dir| dir.join("parakeet-model"))
        && model.is_dir()
    {
        cmd.arg("--model").arg(model);
    }
    cmd.arg("--")
        .arg(audio)
        .stdin(Stdio::null())
        .stdout(Stdio::null());
    let status = run_owned(&mut cmd, cancel, Duration::from_secs(600), None)
        .context("Start the local parakeet-mlx executable")?;
    if !status.success() {
        bail!("Parakeet failed ({status}). Check the local model and ffmpeg.");
    }
    let path = directory.join("transcript.txt");
    std::fs::read_to_string(path).context("Read local Parakeet transcript")
}

#[cfg(test)]
#[path = "voice_tests.rs"]
mod tests;

impl VoiceInput {
    fn start(&mut self, ctx: egui::Context, audio: Option<PathBuf>, source: Option<&str>) {
        self.source = source.map(str::to_owned);
        let (tx, rx) = mpsc::channel();
        let (stop, control) = mpsc::channel();
        let (guard, cancel) = mpsc::channel();
        self.cancel = Some(guard);
        self.error = None;
        self.result = Some(rx);
        self.stop = audio.is_none().then_some(stop);
        std::thread::spawn(move || {
            let result = (|| {
                let temp = tempfile::tempdir()?;
                let file = match audio {
                    Some(path) => path,
                    None => {
                        let wav = temp.path().join("microphone.wav");
                        record(&wav, control, &cancel)?;
                        wav
                    }
                };
                transcribe(&file, temp.path(), &cancel)
            })();
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }

    pub(super) fn show(
        &mut self,
        ui: &mut egui::Ui,
        draft: &mut String,
        focus: &mut bool,
        source: Option<&str>,
    ) {
        self.show_with_label(ui, draft, focus, source, "Parakeet");
    }

    pub(super) fn show_with_label(
        &mut self,
        ui: &mut egui::Ui,
        draft: &mut String,
        focus: &mut bool,
        source: Option<&str>,
        label: &str,
    ) {
        self.completed_command = false;
        if let Some(rx) = &self.result {
            let result = match rx.try_recv() {
                Ok(value) => Some(value),
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err(anyhow::anyhow!("Parakeet worker stopped")))
                }
                Err(mpsc::TryRecvError::Empty) => None,
            };
            if let Some(result) = result {
                self.cancel = None;
                self.result = None;
                self.stop = None;
                match result {
                    Ok(text) => {
                        self.completed_command = draft.trim().is_empty()
                            && source.is_some()
                            && self.source.as_deref() == source;
                        if !draft.is_empty() && !draft.ends_with(char::is_whitespace) {
                            draft.push(' ');
                        }
                        draft.push_str(&text);
                        *focus = true;
                    }
                    Err(err) => self.error = Some(format!("{err:#}")),
                }
            }
        }
        if let Some(stop) = &self.stop {
            let finish = t("chat.voice_finish");
            let button = if label == "🎙" {
                egui::Button::new(egui::RichText::new("■").color(ui.visuals().error_fg_color))
            } else {
                egui::Button::new(&finish)
            };
            if ui.add(button).on_hover_text(&finish).clicked() {
                let _ = stop.send(());
                self.stop = None;
            }
        } else if self.result.is_some() {
            ui.spinner().on_hover_text(t("chat.voice_hint"));
        } else {
            let atoms = if label == "🎙" {
                egui::Atoms::new(octa::ui::icons::icon_image(
                    ui,
                    octa::ui::icons::Icon::Microphone,
                ))
            } else {
                egui::Atoms::new((
                    octa::ui::icons::icon_image(ui, octa::ui::icons::Icon::Microphone),
                    label,
                ))
            };
            ui.menu_button(atoms, |ui| {
                if cfg!(target_os = "macos")
                    && ui
                        .button(t("chat.voice_record"))
                        .on_hover_text(t("chat.voice_hint"))
                        .clicked()
                {
                    ui.close();
                    self.start(ui.ctx().clone(), None, source);
                }
                if ui
                    .button(t("chat.voice_file"))
                    .on_hover_text(t("chat.voice_hint"))
                    .clicked()
                {
                    ui.close();
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter(
                            "Audio",
                            &["wav", "m4a", "mp3", "flac", "aiff", "ogg", "opus"],
                        )
                        .pick_file()
                    {
                        self.start(ui.ctx().clone(), Some(path), source);
                    }
                }
            })
            .response
            .on_hover_text(t("chat.voice_hint"));
        }
        if let Some(error) = &self.error {
            if label == "🎙" {
                ui.colored_label(ui.visuals().error_fg_color, "!")
                    .on_hover_text(error);
            } else {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
        }
    }
}

#[cfg(test)]
#[test]
fn recording_stop_stays_inside_command_slot_and_completes_on_click() {
    let (stop, rx) = mpsc::channel();
    let mut voice = VoiceInput {
        stop: Some(stop),
        ..Default::default()
    };
    let ctx = egui::Context::default();
    let mut frame = |events| {
        let mut output = ctx.run_ui(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ui| {
                ui.horizontal(|ui| {
                    voice.show_with_label(ui, &mut String::new(), &mut false, None, "🎙")
                });
            },
        );
        output.textures_delta.clear();
        output
    };
    let out = frame(vec![]);
    let pos = out
        .shapes
        .iter()
        .find_map(|shape| {
            if let egui::Shape::Text(text) = &shape.shape
                && text.galley.text() == "■"
            {
                assert!(text.galley.size().x < 24.0);
                Some(text.pos + text.galley.size() * 0.5)
            } else {
                None
            }
        })
        .expect("visible compact stop button");
    frame(vec![egui::Event::PointerMoved(pos)]);
    for pressed in [true, false] {
        frame(vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        }]);
    }
    rx.try_recv().expect("recording receives stop action");
}
