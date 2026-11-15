use super::*;

/// Every table of a page opens in its own tab, named after the file and the
/// table's caption. A reader that leaves `source_path` unset (HTML, PDF) still
/// gives its tabs the file: before, they read "Untitled - Products" and Save
/// and Refresh had no file to go back to.
#[test]
fn each_table_of_a_page_opens_in_a_tab_named_after_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("menu.html");
    std::fs::write(
        &path,
        "<table><caption>Products</caption><tr><th>id</th></tr><tr><td>1</td></tr></table>\
         <table><tr><th>city</th></tr><tr><td>Rome</td></tr></table>",
    )
    .unwrap();
    let mut app = OctaApp::new(
        vec![],
        ui::settings::AppSettings::default(),
        ui::settings::IconVariant::Rose,
    );

    app.load_file(path.clone());

    let titles: Vec<String> = app.tabs.iter().map(|t| t.title_display()).collect();
    assert_eq!(titles, ["menu.html - Products", "menu.html - Table 2"]);
    let file = path.to_string_lossy().to_string();
    assert!(
        app.tabs
            .iter()
            .all(|t| t.table.source_path.as_deref() == Some(file.as_str()))
    );
}

/// Exercise the GUI's actual load, smart-filter, edit and Save routes together.
#[test]
fn smart_workbook_edits_the_filtered_row_and_preserves_the_other_sheet() {
    use crate::app::chat::ask_filter;
    use crate::app::state::AskFilterJob;
    use octa::data::CellValue;
    use rust_xlsxwriter::{Formula, Workbook};

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sales.xlsx");
    let mut book = Workbook::new();
    let sheet = book.add_worksheet().set_name("Sales").unwrap();
    sheet.write_string(0, 0, "amount").unwrap();
    sheet.write_number(1, 0, 800).unwrap();
    sheet.write_number(2, 0, 1200).unwrap();
    book.add_worksheet()
        .set_name("Summary")
        .unwrap()
        .write_string(0, 0, "sum")
        .unwrap()
        .write_formula(1, 0, Formula::new("=SUM(Sales!A2:A3)").set_result("2000"))
        .unwrap();
    book.save(&path).unwrap();
    let mut app = OctaApp::new(
        vec![],
        ui::settings::AppSettings::default(),
        ui::settings::IconVariant::Rose,
    );
    app.load_file(path.clone());
    assert_eq!(app.tabs.len(), 2);
    let index = app
        .tabs
        .iter()
        .position(|t| t.sheet_name.as_deref() == Some("Sales"))
        .unwrap();
    assert_eq!(app.active_tab, index);
    let cols = app.tabs[index].table.columns.clone();
    let target = Arc::new(());
    app.tabs[index].ask_filter_target = Some(target.clone());
    let filter = ask_filter::parse_reply(
        r#"{"predicates":[{"column":"amount","op":">","value":1000}]}"#,
        &cols,
    );
    app.ask_filter_job = Some(AskFilterJob {
        target,
        columns: cols,
        original_search: None,
        result: Arc::new(Mutex::new(Some(filter))),
    });
    app.drain_ask_filter();
    app.recompute_filter();
    assert_eq!(app.tabs[index].filtered_rows, [1]);
    let row = app.tabs[index].filtered_rows[0];
    app.tabs[index].table.set(row, 0, CellValue::Float(1300.));
    app.save_tab(index);
    let reader = formats::excel_reader::ExcelReader;
    let sales = reader.read_table(&path, "Sales").unwrap();
    assert_eq!(sales.rows[0][0], CellValue::Float(800.));
    assert_eq!(sales.rows[1][0], CellValue::Float(1300.));
    let summary = reader.read_table(&path, "Summary").unwrap();
    assert_eq!(summary.formula(0, 0), Some("=SUM(Sales!A2:A3)"));
}
