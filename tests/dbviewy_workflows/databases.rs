use super::*;
use octa::formats::{FormatReader, duckdb_reader::DuckDbReader, sqlite_reader::SqliteReader};
use octa::sql::{AttachKind, SqlWorkspace, WriteMode, WriteTarget};

fn sqlite(fx: &Fixture, schema: &str) -> PathBuf {
    let path = fx.path("office.sqlite");
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch(schema)
        .unwrap();
    path
}

fn duck(fx: &Fixture, schema: &str) -> PathBuf {
    let path = fx.path("office.duckdb");
    duckdb::Connection::open(&path)
        .unwrap()
        .execute_batch(schema)
        .unwrap();
    path
}

#[test]
fn uc13_auditor_browses_quoted_sqlite_table_without_touching_the_file() {
    let fx = Fixture::new();
    let path = sqlite(
        &fx,
        r#"CREATE TABLE "Sales 2026" (id INTEGER PRIMARY KEY, amount INTEGER);
           INSERT INTO "Sales 2026" VALUES (4, 280);
           CREATE TABLE archived(id INTEGER); INSERT INTO archived VALUES (1);"#,
    );
    let before = fs::read(&path).unwrap();
    let catalog = SqliteReader.list_tables(&path).unwrap().unwrap();
    assert_eq!(
        catalog.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
        ["Sales 2026", "archived"]
    );
    let table = read_table_auto(&path, Some("Sales 2026"), MAX_DECOMPRESSED).unwrap();
    numbers(&table, "amount", &[280.0]);
    assert_eq!(fs::read(path).unwrap(), before);
}

#[test]
fn uc14_sqlite_correction_fires_the_audit_trigger_only_for_the_edited_row() {
    let fx = Fixture::new();
    let path = sqlite(
        &fx,
        "CREATE TABLE amounts (id INTEGER PRIMARY KEY, amount INTEGER);
         INSERT INTO amounts VALUES (7,20),(8,20),(9,20);
         CREATE TABLE audit (id INTEGER, old_amount INTEGER, new_amount INTEGER);
         CREATE TRIGGER audited AFTER UPDATE ON amounts BEGIN
           INSERT INTO audit VALUES (old.id,old.amount,new.amount);
         END;",
    );
    let mut grid = SqliteReader.read_table(&path, "amounts").unwrap();
    grid.set(1, col(&grid, "amount"), CellValue::Int(31));
    grid.apply_edits();
    SqliteReader.write_file(&path, &grid).unwrap();
    let audit = SqliteReader.read_table(&path, "audit").unwrap();
    numbers(&audit, "id", &[8.0]);
    numbers(&audit, "old_amount", &[20.0]);
    numbers(&audit, "new_amount", &[31.0]);
    numbers(
        &SqliteReader.read_table(&path, "amounts").unwrap(),
        "amount",
        &[20.0, 31.0, 20.0],
    );
}

#[test]
fn uc15_sqlite_invalid_amount_rolls_back_a_batch() {
    let fx = Fixture::new();
    let path = sqlite(
        &fx,
        "CREATE TABLE amounts (id INTEGER PRIMARY KEY, amount INTEGER CHECK (amount>=0));
         INSERT INTO amounts VALUES (1,10),(2,20);",
    );
    let mut grid = SqliteReader.read_table(&path, "amounts").unwrap();
    grid.set(0, col(&grid, "amount"), CellValue::Int(55));
    grid.set(1, col(&grid, "amount"), CellValue::Int(-1));
    grid.apply_edits();
    let error = SqliteReader.write_file(&path, &grid).unwrap_err();
    assert!(format!("{error:#}").contains("CHECK"), "{error:#}");
    numbers(
        &SqliteReader.read_table(&path, "amounts").unwrap(),
        "amount",
        &[10.0, 20.0],
    );
}

#[test]
fn uc16_sqlite_schema_refusal_does_not_commit_pending_value_edits() {
    let fx = Fixture::new();
    let path = sqlite(
        &fx,
        "CREATE TABLE amounts(id INTEGER PRIMARY KEY, amount INT); INSERT INTO amounts VALUES (4,8);",
    );
    let mut grid = SqliteReader.read_table(&path, "amounts").unwrap();
    grid.set(0, col(&grid, "amount"), CellValue::Int(99));
    grid.insert_column(2, "unapproved".into(), "Utf8".into());
    grid.apply_edits();
    let error = SqliteReader.write_file(&path, &grid).unwrap_err();
    assert!(
        error.to_string().to_lowercase().contains("schema"),
        "{error:#}"
    );
    let reloaded = SqliteReader.read_table(&path, "amounts").unwrap();
    assert_eq!(reloaded.col_count(), 2);
    numbers(&reloaded, "amount", &[8.0]);
}

#[test]
fn uc17_sqlite_attachment_bytes_and_nulls_survive_a_label_edit() {
    let fx = Fixture::new();
    let path = sqlite(
        &fx,
        "CREATE TABLE docs (id INTEGER PRIMARY KEY, label TEXT, attachment BLOB);
         INSERT INTO docs VALUES (1,'Draft',X'00ABFF'),(2,'Missing',NULL),(3,'Empty',X'');",
    );
    let mut grid = SqliteReader.read_table(&path, "docs").unwrap();
    grid.set(0, col(&grid, "label"), CellValue::String("Final".into()));
    grid.apply_edits();
    SqliteReader.write_file(&path, &grid).unwrap();
    let reloaded = SqliteReader.read_table(&path, "docs").unwrap();
    text(&reloaded, "label", &["Final", "Missing", "Empty"]);
    let conn = rusqlite::Connection::open(&path).unwrap();
    let mut stmt = conn
        .prepare("SELECT typeof(attachment), hex(attachment) FROM docs ORDER BY id")
        .unwrap();
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        rows,
        [
            ("blob".into(), "00ABFF".into()),
            ("null".into(), "".into()),
            ("blob".into(), "".into())
        ]
    );
}

#[test]
fn uc18_sqlite_query_result_append_respects_the_target_constraint() {
    let fx = Fixture::new();
    let path = sqlite(
        &fx,
        "CREATE TABLE balance(id INTEGER PRIMARY KEY, amount INTEGER CHECK (amount>=0));
         INSERT INTO balance VALUES (1,10);",
    );
    let mut ws = SqlWorkspace::new().unwrap();
    let mut target = WriteTarget {
        path: path.clone(),
        kind: AttachKind::Sqlite,
        schema: None,
        table: "balance".into(),
        mode: WriteMode::Append,
        source_query: "SELECT 2 AS id, 30 AS amount".into(),
        create_schema_if_missing: false,
    };
    assert_eq!(ws.write_result_to_db(&target).unwrap().rows_written, 1);
    target.source_query = "SELECT 3 AS id, 42 AS amount UNION ALL SELECT 4, -2".into();
    assert!(ws.write_result_to_db(&target).is_err());
    let result = SqliteReader.read_table(&path, "balance").unwrap();
    numbers(&result, "id", &[1.0, 2.0]);
    numbers(&result, "amount", &[10.0, 30.0]);
}

#[test]
fn uc19_duckdb_dotted_table_in_named_schema_retains_typed_date() {
    let fx = Fixture::new();
    let path = duck(
        &fx,
        r#"CREATE SCHEMA billing;
        CREATE TABLE billing."2026.invoices"(id INTEGER, booked DATE);
        INSERT INTO billing."2026.invoices" VALUES (1, DATE '2026-02-03');"#,
    );
    let grid = DuckDbReader
        .read_table(&path, "billing.2026.invoices")
        .unwrap();
    numbers(&grid, "id", &[1.0]);
    assert_eq!(
        cell(&grid, 0, "booked"),
        &CellValue::Date("2026-02-03".into())
    );
    assert!(grid.columns.iter().all(|c| c.name != "__octa_row_id"));
}

#[test]
fn uc20_duckdb_reordered_duplicate_values_still_update_by_identity() {
    let fx = Fixture::new();
    let path = duck(
        &fx,
        "CREATE TABLE stock(id INTEGER PRIMARY KEY, qty INTEGER); INSERT INTO stock VALUES (1,5),(2,5),(3,5);",
    );
    let mut grid = DuckDbReader.read_table(&path, "stock").unwrap();
    let id = col(&grid, "id");
    grid.sort_rows_by_columns(&[(id, false)], octa::data::TextOrder::Letters);
    numbers(&grid, "id", &[3.0, 2.0, 1.0]);
    grid.set(0, col(&grid, "qty"), CellValue::Int(12));
    grid.apply_edits();
    DuckDbReader.write_file(&path, &grid).unwrap();
    let conn = duckdb::Connection::open(&path).unwrap();
    let mut q = conn.prepare("SELECT qty FROM stock ORDER BY id").unwrap();
    let values = q
        .query_map([], |row| row.get::<_, i64>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(values, [5, 5, 12]);
}

#[test]
fn uc21_duckdb_reconciling_inventory_does_not_touch_the_receipts_table() {
    let fx = Fixture::new();
    let path = duck(
        &fx,
        "CREATE TABLE stock(id INTEGER, qty INTEGER); INSERT INTO stock VALUES (1,5),(2,6),(3,7);
        CREATE TABLE receipts(id INTEGER); INSERT INTO receipts VALUES (11),(12);",
    );
    let mut stock = DuckDbReader.read_table(&path, "stock").unwrap();
    stock.set(0, col(&stock, "qty"), CellValue::Int(9));
    stock.apply_edits();
    stock.delete_row(1);
    let next = stock.row_count();
    stock.insert_row(next);
    stock.set(next, col(&stock, "id"), CellValue::Int(4));
    stock.set(next, col(&stock, "qty"), CellValue::Int(10));
    stock.apply_edits();
    DuckDbReader.write_file(&path, &stock).unwrap();
    let saved = DuckDbReader.read_table(&path, "stock").unwrap();
    numbers(&saved, "id", &[1.0, 3.0, 4.0]);
    numbers(&saved, "qty", &[9.0, 7.0, 10.0]);
    numbers(
        &DuckDbReader.read_table(&path, "receipts").unwrap(),
        "id",
        &[11.0, 12.0],
    );
}

#[test]
fn uc22_duckdb_constraint_error_preserves_every_uncommitted_row() {
    let fx = Fixture::new();
    let path = duck(
        &fx,
        "CREATE TABLE stock(id INTEGER, qty INTEGER CHECK (qty>=0)); INSERT INTO stock VALUES (1,10),(2,20);",
    );
    let mut grid = DuckDbReader.read_table(&path, "stock").unwrap();
    grid.set(0, col(&grid, "qty"), CellValue::Int(42));
    grid.set(1, col(&grid, "qty"), CellValue::Int(-1));
    grid.apply_edits();
    let error = DuckDbReader.write_file(&path, &grid).unwrap_err();
    assert!(
        format!("{error:#}").to_lowercase().contains("constraint"),
        "{error:#}"
    );
    numbers(
        &DuckDbReader.read_table(&path, "stock").unwrap(),
        "qty",
        &[10.0, 20.0],
    );
}

#[test]
fn uc23_duckdb_builds_a_report_in_a_new_schema_not_the_input_csv() {
    let fx = Fixture::new();
    let sales = b"region,qty\nEast,6\nEast,7\nWest,12\n";
    let path = fx.file("sales.csv", sales);
    let warehouse = duck(
        &fx,
        "CREATE TABLE existing(id INTEGER); INSERT INTO existing VALUES (9);",
    );
    let mut ws = SqlWorkspace::new().unwrap();
    ws.set_active_table(&read(&path)).unwrap();
    let result = ws
        .write_result_to_db(&WriteTarget {
            path: warehouse.clone(),
            kind: AttachKind::DuckDb,
            schema: Some("reports".into()),
            table: "summary".into(),
            mode: WriteMode::Create,
            source_query:
                "SELECT region,sum(qty) AS total FROM data GROUP BY region ORDER BY region".into(),
            create_schema_if_missing: true,
        })
        .unwrap();
    assert_eq!(result.rows_written, 2);
    assert!(result.created_schema && !result.cut);
    let saved = DuckDbReader
        .read_table(&warehouse, "reports.summary")
        .unwrap();
    text(&saved, "region", &["East", "West"]);
    text(&saved, "total", &["13", "12"]);
    numbers(
        &DuckDbReader.read_table(&warehouse, "existing").unwrap(),
        "id",
        &[9.0],
    );
    assert_eq!(fs::read(&path).unwrap(), sales);
}

#[test]
fn uc24_attach_duckdb_joins_a_file_with_an_in_memory_batch() {
    let fx = Fixture::new();
    let path = duck(
        &fx,
        "CREATE TABLE catalogue(id INTEGER, sku VARCHAR); INSERT INTO catalogue VALUES (1,'A-1'),(2,'B-2');",
    );
    let mut ws = SqlWorkspace::new().unwrap();
    ws.set_active_table(&read(&fx.file("orders.csv", "id,qty\n2,4\n1,9\n3,2\n")))
        .unwrap();
    let attached = ws.attach(&path, "warehouse", AttachKind::DuckDb).unwrap();
    assert!(attached.native);
    let grid = ws.execute(
        "SELECT c.sku,d.qty FROM data d JOIN warehouse.main.catalogue c USING(id) ORDER BY d.id",
    ).unwrap().table;
    text(&grid, "sku", &["A-1", "B-2"]);
    numbers(&grid, "qty", &[9.0, 4.0]);
}
