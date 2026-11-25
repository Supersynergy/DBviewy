use super::*;
use serde_json::{Value, json};
use std::ffi::OsStr;
use std::process::{Command, Output};

fn cli(fx: &Fixture, args: &[&OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_octa"))
        .args(args)
        .env("OCTA_CONFIG_DIR", fx.path("config"))
        .env_remove("OCTA_JEV_ENABLED")
        .current_dir(fx.0.path())
        .output()
        .expect("spawn DBviewy CLI from this build")
}

fn ok(output: Output) -> Vec<u8> {
    assert!(
        output.status.success(),
        "{:?}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

#[test]
fn uc45_automation_converts_a_supplier_csv_to_machine_readable_jsonl() {
    let fx = Fixture::new();
    let source = b"id,sku,qty\n7,Q-7,3\n8,Q-8,11\n";
    let input = fx.file("delivery.csv", source);
    let out = fx.path("delivery.jsonl");
    ok(cli(
        &fx,
        &["--convert".as_ref(), input.as_os_str(), out.as_os_str()],
    ));
    let rows: Vec<Value> = fs::read_to_string(&out)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        rows,
        [
            json!({"id":7,"sku":"Q-7","qty":3}),
            json!({"id":8,"sku":"Q-8","qty":11})
        ]
    );
    assert_eq!(fs::read(input).unwrap(), source);
}

#[test]
fn uc46_automation_json_preview_sorts_before_applying_the_row_limit() {
    let fx = Fixture::new();
    let input = fx.file("backlog.csv", "id,score\n1,9\n2,15\n3,11\n");
    let out = ok(cli(
        &fx,
        &[
            "-f".as_ref(),
            "json".as_ref(),
            "--head".as_ref(),
            input.as_os_str(),
            "--sort-by".as_ref(),
            "score".as_ref(),
            "--sort-desc".as_ref(),
            "score".as_ref(),
            "-n".as_ref(),
            "2".as_ref(),
        ],
    ));
    let result: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(result, json!([{"id":2,"score":15},{"id":3,"score":11}]));
}

#[test]
fn uc47_automation_writes_grouped_sql_results_to_a_real_sqlite_file() {
    let fx = Fixture::new();
    let input = fx.file("orders.csv", "city,qty\nBerlin,2\nHamburg,5\nBerlin,4\n");
    let out = fx.path("summary.sqlite");
    ok(cli(
        &fx,
        &[
            "--sql".as_ref(),
            input.as_os_str(),
            "--query".as_ref(),
            "SELECT city,sum(qty) AS total FROM data GROUP BY city ORDER BY city".as_ref(),
            "--sql-write-to".as_ref(),
            out.as_os_str(),
            "--sql-write-table".as_ref(),
            "totals".as_ref(),
        ],
    ));
    let result = octa::formats::sqlite_reader::SqliteReader
        .read_table(&out, "totals")
        .unwrap();
    text(&result, "city", &["Berlin", "Hamburg"]);
    // DuckDB SUM(INTEGER) yields HUGEINT; SQLite carries the exact digits as text.
    text(&result, "total", &["6", "5"]);
}

#[test]
fn uc48_automation_schema_gate_accepts_good_input_and_reports_drift() {
    let fx = Fixture::new();
    let good = fx.file("required.csv", "id,amount\n1,15\n");
    let bad = fx.file("drift.csv", "id,note\n1,missing amount\n");
    let schema = ok(cli(
        &fx,
        &[
            "--export-schema".as_ref(),
            good.as_os_str(),
            "-t".as_ref(),
            "json-schema".as_ref(),
        ],
    ));
    let expected = fx.file("expected.json", schema);
    assert!(
        cli(
            &fx,
            &[
                "--validate-schema".as_ref(),
                good.as_os_str(),
                "--expect-schema".as_ref(),
                expected.as_os_str(),
            ]
        )
        .status
        .success()
    );
    let rejected = cli(
        &fx,
        &[
            "--validate-schema".as_ref(),
            bad.as_os_str(),
            "--expect-schema".as_ref(),
            expected.as_os_str(),
        ],
    );
    assert_eq!(rejected.status.code(), Some(1));
    let report = String::from_utf8(rejected.stdout).unwrap();
    assert!(
        report.contains("amount") && report.contains("note"),
        "{report}"
    );
}

#[test]
fn uc49_automation_dedupes_latest_updates_and_publishes_a_reopenable_artifact() {
    let fx = Fixture::new();
    let input = fx.file("contacts.csv", "account,stage\nA,draft\nB,open\nA,won\n");
    let filtered = ok(cli(
        &fx,
        &[
            "--dedupe".as_ref(),
            input.as_os_str(),
            "--dedupe-on".as_ref(),
            "account".as_ref(),
            "--dedupe-keep".as_ref(),
            "last".as_ref(),
            "-f".as_ref(),
            "csv".as_ref(),
        ],
    ));
    let export = fx.file("latest.csv", filtered);
    let raw = ok(cli(
        &fx,
        &[
            "--convert".as_ref(),
            export.as_os_str(),
            "-".as_ref(),
            "--to".as_ref(),
            "json".as_ref(),
        ],
    ));
    let rows: Vec<Value> = serde_json::from_slice(&raw).unwrap();
    assert_eq!(rows.len(), 2, "one record per account");
    assert!(rows.contains(&json!({"account":"A","stage":"won"})));
    assert!(rows.contains(&json!({"account":"B","stage":"open"})));
}

#[test]
fn uc50_automation_bad_sql_does_not_replace_the_last_good_database() {
    let fx = Fixture::new();
    let input = fx.file("orders.csv", "id,qty\n1,2\n");
    let out = fx.path("daily.sqlite");
    {
        let conn = rusqlite::Connection::open(&out).unwrap();
        conn.execute_batch("CREATE TABLE daily(id INTEGER); INSERT INTO daily VALUES (71);")
            .unwrap();
    }
    let before = fs::read(&out).unwrap();
    let bad = cli(
        &fx,
        &[
            "--sql".as_ref(),
            input.as_os_str(),
            "--query".as_ref(),
            "SELECT missing_qty FROM data".as_ref(),
            "--sql-write-to".as_ref(),
            out.as_os_str(),
            "--sql-write-table".as_ref(),
            "daily".as_ref(),
        ],
    );
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("missing_qty"));
    assert_eq!(fs::read(&out).unwrap(), before);
    let table = octa::formats::sqlite_reader::SqliteReader
        .read_table(&out, "daily")
        .unwrap();
    numbers(&table, "id", &[71.0]);
}

use octa::formats::FormatReader;
