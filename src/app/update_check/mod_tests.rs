//! Unit tests for [`mod`](mod). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;

#[test]
fn sha256_hex_matches_known_vector() {
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn parse_sha256sums_accepts_plain_and_binary_marked_lines() {
    let text = "\
0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef  octa-1.0-linux-x86_64.tar.gz
fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210 *octa-1.0-windows-x86_64.zip
not a checksum line
deadbeef  too_short_hash.txt
";
    let sums = parse_sha256sums(text);
    assert_eq!(sums.len(), 2);
    assert_eq!(
        sums.get("octa-1.0-linux-x86_64.tar.gz").map(String::as_str),
        Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
    );
    assert_eq!(
        sums.get("octa-1.0-windows-x86_64.zip").map(String::as_str),
        Some("fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210")
    );
}

#[test]
fn parse_sha256sums_lowercases_hashes() {
    let text = "ABC3456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF  x.zip\n";
    let sums = parse_sha256sums(text);
    assert_eq!(
        sums.get("x.zip").map(String::as_str),
        Some("abc3456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
    );
}

#[test]
fn parse_release_reads_the_tag_from_the_redirect() {
    for (location, want) in [
        (
            "https://github.com/Supersynergy/DBviewy/releases/tag/0.21.2",
            "0.21.2",
        ),
        (
            "https://github.com/Supersynergy/DBviewy/releases/tag/v0.17.0",
            "0.17.0",
        ),
    ] {
        assert_eq!(parse_release(location).unwrap(), want, "for {location}");
    }
}

#[test]
fn parse_release_rejects_an_address_without_a_tag() {
    // A repository with no release yet redirects to the release list.
    assert!(parse_release("https://github.com/Supersynergy/DBviewy/releases").is_err());
    assert!(parse_release("https://github.com/Supersynergy/DBviewy/releases/tag/").is_err());
}

#[test]
fn only_a_higher_version_is_an_update() {
    // The copy already on the newest release, or ahead of what GitHub calls
    // latest, must hear nothing.
    assert!(!is_newer("0.21.2", "0.21.2"));
    assert!(!is_newer("0.21.2", "0.21.3"));
    assert!(!is_newer("0.21.9", "0.21.10"));
    assert!(is_newer("0.21.3", "0.21.2"));
    assert!(is_newer("0.21.10", "0.21.9"));
    assert!(is_newer("0.22.0", "0.21.9"));
    assert!(!is_newer("0.21.9", "0.22.0"));
    assert!(is_newer("1.0.0", "0.99.99"));
}

#[cfg(target_os = "macos")]
#[test]
fn mac_update_dialog_opens_only_dbviewy_downloads_without_copying_a_command() {
    use crate::app::dialogs::update_dialog::render_update_dialog;
    use octa::ui::settings::{AppSettings, IconVariant};

    const RELEASES: &str = "https://github.com/Supersynergy/DBviewy/releases";

    fn hyperlink_in_shape(shape: &egui::Shape) -> Option<egui::Pos2> {
        match shape {
            egui::Shape::Text(text) if text.galley.text() == RELEASES => {
                Some(text.pos + text.galley.size() * 0.5)
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(hyperlink_in_shape),
            _ => None,
        }
    }

    for (name, state) in [
        ("up to date", UpdateState::UpToDate),
        (
            "update available",
            UpdateState::Available {
                version: String::from("0.22.2"),
            },
        ),
        (
            "no final release yet",
            UpdateState::Error(String::from("No final release yet")),
        ),
    ] {
        let ctx = egui::Context::default();
        let mut app = OctaApp::new(vec![], AppSettings::default(), IconVariant::Rose);
        app.show_update_dialog = true;
        *app.update_state.lock().unwrap() = state.clone();
        let mut frame = |events| {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 650.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| render_update_dialog(&mut app, ui.ctx()),
            );
            // This headless frame does not have a renderer to apply textures.
            out.textures_delta.clear();
            out
        };
        frame(vec![]);
        let drawn = frame(vec![]);
        let pos = drawn
            .shapes
            .iter()
            .find_map(|s| hyperlink_in_shape(&s.shape))
            .unwrap_or_else(|| panic!("missing official downloads link for {name}"));
        let pointer = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(vec![egui::Event::PointerMoved(pos)]);
        frame(vec![pointer(true)]);
        let clicked = frame(vec![pointer(false)]);
        let commands = &clicked.platform_output.commands;
        let opened: Vec<_> = commands
            .iter()
            .filter_map(|command| match command {
                egui::OutputCommand::OpenUrl(url) => Some(url.url.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(opened, [RELEASES], "incorrect download destination for {name}");
        assert!(
            commands
                .iter()
                .all(|cmd| !matches!(cmd, egui::OutputCommand::CopyText(_)))
        );
    }
}
