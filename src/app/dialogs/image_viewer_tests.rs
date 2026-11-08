use super::*;

#[test]
fn fit_zoom_contains_the_picture() {
    // Wider viewport than picture: height is the limit.
    let z = fit_zoom(egui::vec2(800.0, 400.0), egui::vec2(200.0, 200.0));
    assert!((z - 2.0).abs() < 1e-6);
    // Taller viewport: width is the limit.
    let z = fit_zoom(egui::vec2(100.0, 800.0), egui::vec2(200.0, 200.0));
    assert!((z - 0.5).abs() < 1e-6);
    // Aspect is kept: a 2:1 picture in a square viewport scales by width.
    let z = fit_zoom(egui::vec2(400.0, 400.0), egui::vec2(400.0, 200.0));
    assert!((z - 1.0).abs() < 1e-6);
}

#[test]
fn fit_zoom_never_breaks_on_empty_input() {
    assert_eq!(fit_zoom(egui::vec2(0.0, 0.0), egui::vec2(10.0, 10.0)), 1.0);
    assert_eq!(
        fit_zoom(egui::vec2(100.0, 100.0), egui::vec2(0.0, 10.0)),
        1.0
    );
    // A tiny picture still gets a usable, positive scale.
    assert!(fit_zoom(egui::vec2(100.0, 100.0), egui::vec2(1.0, 1.0)) > 0.0);
}
