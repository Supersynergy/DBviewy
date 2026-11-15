//! An indexed seek must keep an in-field UTF-8 BOM as data.
use std::fs;
use std::process::Command;

#[test]
fn indexed_slice_preserves_bom_inside_first_field() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("orders.csv");
    fs::write(&path, b"id\nx\n\xef\xbb\xbffoo\n").unwrap();
    let offsets: Vec<u8> = [0u64, 3, 5, 3]
        .into_iter()
        .flat_map(u64::to_be_bytes)
        .collect();
    fs::write(path.with_extension("csv.idx"), offsets).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_octa-csv"))
        .args(["--indexed", "slice"])
        .arg(path)
        .args(["1", "1"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"id\n\xef\xbb\xbffoo\n");
}

#[test]
fn an_index_pointing_into_a_field_is_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("orders.csv");
    fs::write(&path, b"id\nhello\nworld\n").unwrap();
    let offsets: Vec<u8> = [0u64, 3, 10, 3]
        .into_iter()
        .flat_map(u64::to_be_bytes)
        .collect();
    fs::write(path.with_extension("csv.idx"), offsets).unwrap();
    for args in [vec!["verify-index"], vec!["--indexed", "slice"]] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_octa-csv"));
        command.args(&args).arg(&path);
        if args.len() == 2 {
            command.args(["1", "1"]);
        }
        let output = command.output().unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
    }
}
