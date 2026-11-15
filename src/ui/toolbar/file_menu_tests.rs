use super::recent_file_label;

#[test]
fn recent_files_disambiguate_duplicate_basenames() {
    let recent: Vec<String> = vec![
        "/a/data.csv".into(),
        "/b/data.csv".into(),
        "/a/unique.csv".into(),
    ];
    assert_eq!(recent_file_label(&recent[0], &recent), "data.csv (/a)");
    assert_eq!(recent_file_label(&recent[1], &recent), "data.csv (/b)");
    assert_eq!(recent_file_label(&recent[2], &recent), "unique.csv");
}

#[test]
fn copied_source_keeps_the_target_and_expands_relative_paths() {
    assert_eq!(
        super::clipboard_source("leads/Berlin.xlsx", true),
        "Berlin.xlsx"
    );
    assert_eq!(
        super::clipboard_source("leads/Berlin.xlsx", false),
        std::env::current_dir()
            .unwrap()
            .join("leads/Berlin.xlsx")
            .to_string_lossy()
    );
    assert_eq!(
        super::clipboard_source("https://example.org/Berlin.xlsx", false),
        "https://example.org/Berlin.xlsx"
    );
}
