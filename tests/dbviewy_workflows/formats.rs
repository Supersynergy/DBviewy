use super::*;
use std::io::Write as _;

use octa::formats::excel_reader::OpenExcelWorkbook;
use octa::sql::run_query;

#[test]
fn uc01_supplier_csv_with_multiline_notes_survives_parquet_export() {
    let fx = Fixture::new();
    let source = fx.file(
        "supplier.csv",
        "sku;notes;items\r\nA-1;\"first line\nwith;semicolon\";12\r\nB-2;ready;4\r\n",
    );
    let out = fx.path("supplier.parquet");
    write(&out, &read(&source));
    let result = read(&out);
    text(&result, "sku", &["A-1", "B-2"]);
    text(&result, "notes", &["first line\nwith;semicolon", "ready"]);
    numbers(&result, "items", &[12.0, 4.0]);
}

#[test]
fn uc02_multilingual_tsv_keeps_a_missing_value_in_json_lines() {
    let fx = Fixture::new();
    let source = fx.file("leads.tsv", "city\tamount\nZürich\t15\n東京\t\n");
    let path = fx.path("leads.jsonl");
    write(&path, &read(&source));
    let json: Vec<serde_json::Value> = fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        json,
        [
            serde_json::json!({"city":"Zürich","amount":15}),
            serde_json::json!({"city":"東京","amount":null}),
        ]
    );
}

#[test]
fn uc03_sparse_json_records_export_for_spreadsheet_review() {
    let fx = Fixture::new();
    let source = fx.file(
        "contacts.json",
        r#"[{"id":7,"mail":"a@example.test"},{"id":8}]"#,
    );
    let out = fx.path("contacts.tsv");
    write(&out, &read(&source));
    let reopened = read(&out);
    numbers(&reopened, "id", &[7.0, 8.0]);
    assert_eq!(
        cell(&reopened, 0, "mail"),
        &CellValue::String("a@example.test".into())
    );
    assert_eq!(cell(&reopened, 1, "mail"), &CellValue::Null);
}

#[test]
fn uc04_json_event_batch_keeps_flags_and_row_order_in_arrow() {
    let fx = Fixture::new();
    let source = fx.file(
        "events.jsonl",
        "{\"event\":\"paid\",\"ok\":true}\n{\"event\":\"void\",\"ok\":false}\n",
    );
    let out = fx.path("events.arrow");
    write(&out, &read(&source));
    let result = read(&out);
    text(&result, "event", &["paid", "void"]);
    assert_eq!(
        cells(&result, "ok"),
        [CellValue::Bool(true), CellValue::Bool(false)]
    );
}

#[test]
fn uc05_parquet_inventory_filter_yields_a_reopenable_csv() {
    let fx = Fixture::new();
    let path = fx.path("inventory.parquet");
    write(
        &path,
        &read(&fx.file("inventory.csv", "sku,stock\nA-1,0\nB-2,18\nC-3,2\n")),
    );
    let selected = run_query(
        &read(&path),
        "SELECT * FROM data WHERE stock > 0 ORDER BY stock",
    )
    .unwrap();
    let out = fx.path("available.csv");
    write(&out, &selected.table);
    let result = read(&out);
    text(&result, "sku", &["C-3", "B-2"]);
    numbers(&result, "stock", &[2.0, 18.0]);
}

#[test]
fn uc06_arrow_handoff_retains_nulls_in_grouped_analysis() {
    let fx = Fixture::new();
    let path = fx.path("inspection.arrow");
    write(
        &path,
        &read(&fx.file("inspection.csv", "status,cost\nopen,3\nclosed,2\nopen,\n")),
    );
    let result = run_query(
        &read(&path),
        "SELECT status, count(*) AS records, count(cost) AS priced FROM data GROUP BY status ORDER BY status",
    ).unwrap().table;
    text(&result, "status", &["closed", "open"]);
    numbers(&result, "records", &[1.0, 2.0]);
    numbers(&result, "priced", &[1.0, 1.0]);
}

#[test]
fn uc07_named_excel_sheet_exports_without_the_other_sheet() {
    let fx = Fixture::new();
    let path = fx.path("regional.xlsx");
    let mut book = rust_xlsxwriter::Workbook::new();
    let ignored = book.add_worksheet().set_name("Archive").unwrap();
    ignored.write_string(0, 0, "region").unwrap();
    ignored.write_string(1, 0, "Archive only").unwrap();
    let wanted = book.add_worksheet().set_name("Sales & service").unwrap();
    wanted.write_string(0, 0, "region").unwrap();
    wanted.write_string(0, 1, "amount").unwrap();
    wanted.write_string(1, 0, "Basel").unwrap();
    wanted.write_number(1, 1, 42.0).unwrap();
    book.save(&path).unwrap();

    let mut book = OpenExcelWorkbook::open(&path).unwrap();
    assert_eq!(book.sheet_names(), ["Archive", "Sales & service"]);
    let out = fx.path("active-sheet.csv");
    write(&out, &book.read_sheet("Sales & service").unwrap());
    let values = read(&out);
    text(&values, "region", &["Basel"]);
    numbers(&values, "amount", &[42.0]);
    assert!(book.read_sheet("Missing").is_err());
}

#[test]
fn uc08_ods_reopen_preserves_workbook_text_and_amounts() {
    let fx = Fixture::new();
    let input = read(&fx.file("quotes.csv", "item,amount\nCafé & Tee,29\nBüro,11\n"));
    let path = fx.path("quotes.ods");
    write(&path, &input);
    let reopened = read(&path);
    text(&reopened, "item", &["Café & Tee", "Büro"]);
    numbers(&reopened, "amount", &[29.0, 11.0]);
}

#[test]
fn uc09_gzipped_report_retains_provenance_and_exported_rows() {
    let fx = Fixture::new();
    let path = fx.path("stock.csv.gz");
    let mut stream = flate2::write::GzEncoder::new(
        fs::File::create(&path).unwrap(),
        flate2::Compression::fast(),
    );
    stream.write_all(b"sku,stock\nA-1,3\nB-2,7\n").unwrap();
    stream.finish().unwrap();
    let table = read(&path);
    assert_eq!(table.source_path.as_deref(), path.to_str());
    let out = fx.path("normalised.tsv");
    write(&out, &table);
    text(&read(&out), "sku", &["A-1", "B-2"]);
    numbers(&read(&out), "stock", &[3.0, 7.0]);
}

#[test]
fn uc10_yaml_service_list_enters_a_csv_pipeline() {
    let fx = Fixture::new();
    let path = fx.file(
        "services.yaml",
        "- service: mail\n  port: 25\n- service: database\n  port: 5432\n",
    );
    let out = fx.path("services.csv");
    write(&out, &read(&path));
    let result = read(&out);
    text(&result, "service", &["mail", "database"]);
    numbers(&result, "port", &[25.0, 5432.0]);
}

#[test]
fn uc11_zstd_event_batch_refuses_a_cap_then_reads_the_unchanged_source() {
    let fx = Fixture::new();
    let input = "{\"sku\":\"longer stock entry\",\"stock\":7}\n{\"sku\":\"B\",\"stock\":2}\n";
    let data = zstd::stream::encode_all(input.as_bytes(), 1).unwrap();
    let path = fx.file("stock.jsonl.zst", &data);
    assert!(read_table_auto(&path, None, 16).is_err());
    assert_eq!(fs::read(&path).unwrap(), data);
    numbers(&read(&path), "stock", &[7.0, 2.0]);
}

#[test]
fn uc12_avro_archive_keeps_a_decimal_beyond_binary_float_precision() {
    let fx = Fixture::new();
    let path = fx.file(
        "payments.csv",
        "id,amount\n1,12345678901234567890.12\n2,0.25\n",
    );
    let out = fx.path("payments.avro");
    write(&out, &read(&path));
    let result = read(&out);
    numbers(&result, "id", &[1.0, 2.0]);
    assert_eq!(
        cell(&result, 0, "amount").to_string(),
        "12345678901234567890.12"
    );
    assert!(
        result.columns[col(&result, "amount")]
            .data_type
            .starts_with("Decimal")
    );
}
