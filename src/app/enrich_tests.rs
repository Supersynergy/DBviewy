use super::*;

fn dlg(mode: GuiExtract) -> EnrichDialogState {
    EnrichDialogState {
        col_idx: 0,
        mode,
        target_buf: "url_live".into(),
        pattern_buf: String::new(),
        limit_buf: "500".into(),
        top_k_buf: "3".into(),
        cell_chars_buf: "500".into(),
        provenance: true,
        refresh: false,
        only_missing: false,
        error: None,
    }
}

#[test]
fn defaults_build_status_params() {
    let p = params_from(&dlg(GuiExtract::Status), "url").unwrap();
    assert_eq!(p.open_tab, "@active");
    assert!(matches!(p.url_column, ColRef::Name(ref n) if n == "url"));
    assert_eq!(p.target_column, "url_live");
    assert!(matches!(p.extract, Extract::Status));
    assert_eq!(p.limit, Some(500));
    assert_eq!(p.top_k, Some(3));
    assert_eq!(p.cell_chars, Some(500));
    assert!(p.provenance && !p.refresh && !p.only_missing);
    assert!(p.pattern.is_none());
}

#[test]
fn empty_target_is_refused() {
    let mut d = dlg(GuiExtract::Status);
    d.target_buf = "   ".into();
    assert!(params_from(&d, "url").is_err());
}

#[test]
fn pattern_mode_needs_a_pattern() {
    let d = dlg(GuiExtract::Pattern);
    assert!(params_from(&d, "url").is_err());
    let mut d2 = dlg(GuiExtract::Pattern);
    d2.pattern_buf = "([0-9]+)".into();
    let p = params_from(&d2, "url").unwrap();
    assert_eq!(p.pattern.as_deref(), Some("([0-9]+)"));
}

#[test]
fn bad_regex_is_refused_before_the_worker() {
    let mut d = dlg(GuiExtract::Search);
    d.pattern_buf = "([0-9]+".into();
    let err = params_from(&d, "url").unwrap_err();
    assert!(!err.is_empty());
}

#[test]
fn numeric_buffers_fall_back_and_clamp() {
    let mut d = dlg(GuiExtract::Search);
    d.limit_buf = "abc".into();
    d.top_k_buf = "x".into();
    d.cell_chars_buf = "0".into();
    let p = params_from(&d, "url").unwrap();
    assert_eq!(p.limit, Some(500));
    assert_eq!(p.top_k, None);
    assert_eq!(p.cell_chars, None);
    d.limit_buf = "99999".into();
    let p2 = params_from(&d, "url").unwrap();
    assert_eq!(p2.limit, Some(500));
}

#[test]
fn mode_maps_to_extract() {
    assert!(matches!(
        params_from(&dlg(GuiExtract::Text), "url").unwrap().extract,
        Extract::Text
    ));
    assert!(matches!(
        params_from(&dlg(GuiExtract::Search), "url")
            .unwrap()
            .extract,
        Extract::Search
    ));
}
