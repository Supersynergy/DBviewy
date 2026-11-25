//! Run the README's actual sample text against production readers, formulas and CLI.
use std::path::{Path, PathBuf};
use std::process::Command;

use octa::data::{BinaryDisplayMode, CellValue};
use octa::formats::{FormatReader, csv_reader::CsvReader, excel_reader::ExcelReader};
use octa::ui::table_view::commit_cell_text;

const README: &str = include_str!("../README.md");
const SAMPLE: &str = "samples/features/dbviewy-orders.csv";
const ORIGINAL: &[u8] = include_bytes!("../samples/features/dbviewy-orders.csv");

fn quickstart() -> &'static str {
    README
        .split_once("<!-- dbviewy-quickstart -->")
        .unwrap()
        .1
        .split_once("<!-- /dbviewy-quickstart -->")
        .unwrap()
        .0
}

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn sample_copy(dir: &Path) -> PathBuf {
    let source = dir.join("orders.csv");
    std::fs::write(&source, ORIGINAL).unwrap();
    source
}

fn number(cell: Option<&CellValue>) -> f64 {
    match cell {
        Some(CellValue::Int(n)) => *n as f64,
        Some(CellValue::Float(n)) => *n,
        other => panic!("expected a numerical result, got {other:?}"),
    }
}

#[test]
fn quickstart_readme_formulas_survive_xlsx_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let source = sample_copy(dir.path());
    let mut table = CsvReader.read_file(&source).unwrap();
    assert_eq!(table.row_count(), 6);
    assert_eq!(table.columns[4].name, "result");
    let mut observed = Vec::new();
    for line in quickstart().lines().filter(|line| line.starts_with("| `E")) {
        let columns: Vec<_> = line.trim_matches('|').split('|').map(str::trim).collect();
        let [address, example, expected] = columns.as_slice() else {
            panic!("bad quickstart example row: {line}");
        };
        let row = address
            .strip_prefix("`E")
            .and_then(|address| address.strip_suffix('`'))
            .unwrap()
            .parse::<usize>()
            .unwrap()
            - 1;
        let example = example.trim_matches('`');
        assert!(example.starts_with('='));
        assert!(commit_cell_text(
            &mut table,
            row,
            4,
            example.to_owned(),
            BinaryDisplayMode::default()
        ));
        let value = number(table.get(row, 4));
        assert_eq!(value, expected.parse::<f64>().unwrap(), "{example}");
        observed.push((row, value));
    }
    assert_eq!(observed, [(0, 148.5), (1, 2.0), (2, 59.0)]);
    let xlsx = dir.path().join("orders-check.xlsx");
    ExcelReader.write_file(&xlsx, &table).unwrap();
    let reopened = ExcelReader.read_file(&xlsx).unwrap();
    assert_eq!(reopened.row_count(), 6);
    assert_eq!(reopened.columns[4].name, "result");
    for (row, value) in observed {
        assert_eq!(number(reopened.get(row, 4)), value, "export row {row}");
    }
    assert!(
        reopened.formulas.is_empty(),
        "this exports results, not formulas"
    );
    assert_eq!(std::fs::read(source).unwrap(), ORIGINAL);
}

#[test]
fn quickstart_readme_sql_returns_the_same_paid_total() {
    let script = quickstart()
        .split_once("```sh\n")
        .unwrap()
        .1
        .split_once("\n```")
        .unwrap()
        .0;
    let mut lines = script.lines();
    assert_eq!(
        lines.next(),
        Some("cargo run --release --locked --bin octa -- \\")
    );
    let flags: Vec<_> = lines
        .next()
        .unwrap()
        .trim()
        .strip_suffix('\\')
        .unwrap()
        .split_ascii_whitespace()
        .collect();
    assert_eq!(flags, ["--sql", SAMPLE, "-f", "json"]);
    let query = lines
        .next()
        .unwrap()
        .trim()
        .strip_prefix("-q \"")
        .unwrap()
        .strip_suffix('"')
        .unwrap();
    assert!(lines.next().is_none());
    let config = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_octa"))
        .current_dir(root())
        .env("OCTA_CONFIG_DIR", config.path())
        .env_remove("OCTA_JEV_ENABLED")
        .args(flags)
        .args(["-q", query])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result, serde_json::json!([{"paid_west":148.5}]));
    assert_eq!(std::fs::read(root().join(SAMPLE)).unwrap(), ORIGINAL);
}
