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
