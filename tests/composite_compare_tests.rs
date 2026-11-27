//! The CLI must use exact field boundaries for synthetic CSV keys.

use std::path::Path;
use std::process::Command;

const K1: [&str; 2] = ["Kühlung\u{1f}West", "Tor\nA"];
const K2: [&str; 2] = ["Kühlung", "West\u{1f}Tor\nA"];

fn write(path: &Path, rows: &[([&str; 2], &str, &str)]) {
    let mut writer = csv::Writer::from_path(path).unwrap();
    writer
        .write_record(["Zone", "Teil", "Menge", "Notiz"])
        .unwrap();
    for (key, qty, note) in rows {
        writer.write_record([key[0], key[1], qty, note]).unwrap();
    }
    writer.flush().unwrap();
}

#[test]
fn cli_set_and_keyed_diff_do_not_match_distinct_composite_csv_rows() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("vorher.csv");
    let b = dir.path().join("nachher.csv");
    write(&a, &[(K1, "10", "gleich")]);
    write(&b, &[(K2, "10", "gleich")]);
    for (mode, options) in [("set", &[][..]), ("join", &["--diff-on", "Zone,Teil"][..])] {
        let output = Command::new(env!("CARGO_BIN_EXE_octa"))
            .arg("--diff")
            .arg(&a)
            .arg(&b)
            .args(["--diff-mode", mode, "-f", "csv"])
            .args(options)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut reader = csv::Reader::from_reader(output.stdout.as_slice());
        let header = reader.headers().unwrap();
        let zone = header.iter().position(|col| col == "Zone").unwrap();
        let part = header.iter().position(|col| col == "Teil").unwrap();
        let records = reader.records().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(records.len(), 2, "{mode}");
        for (i, key) in [K1, K2].into_iter().enumerate() {
            assert_eq!([&records[i][zone], &records[i][part]], key);
        }
        assert_eq!(&records[0][0], "only_in_a");
        assert_eq!(&records[1][0], "only_in_b");
    }
}

#[test]
fn cli_three_way_merge_keeps_independent_edits_after_a_composite_key_reorder() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("original.csv");
    let a = dir.path().join("version-a.csv");
    let b = dir.path().join("version-b.csv");
    let result = dir.path().join("merged.csv");
    write(&base, &[(K1, "10", "A"), (K2, "20", "B")]);
    write(&a, &[(K2, "20", "B1"), (K1, "11", "A")]);
    write(&b, &[(K1, "10", "A2"), (K2, "22", "B")]);
    let preserved = [&base, &a, &b].map(|path| std::fs::read(path).unwrap());

    let output = Command::new(env!("CARGO_BIN_EXE_octa"))
        .arg("--merge")
        .arg(&a)
        .arg(&b)
        .arg("--merge-original")
        .arg(&base)
        .args(["--merge-key", "Zone,Teil", "--merge-out"])
        .arg(&result)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut reader = csv::Reader::from_path(result).unwrap();
    let records = reader.records().collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(records.len(), 2);
    for (row, expected) in records
        .iter()
        .zip([[K1[0], K1[1], "11", "A2"], [K2[0], K2[1], "22", "B1"]])
    {
        assert_eq!(row.iter().collect::<Vec<_>>(), expected);
    }
    assert_eq!(
        [&base, &a, &b].map(|path| std::fs::read(path).unwrap()),
        preserved
    );
}
