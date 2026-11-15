use rust_xlsxwriter::Workbook;
use std::process::Command;

#[test]
fn preview_keeps_sparse_coordinates_and_text_identifiers() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("orders.xlsx");
    let mut book = Workbook::new();
    book.add_worksheet().write_string(0, 0, "First").unwrap();
    let sheet = book.add_worksheet().set_name("Leads").unwrap();
    sheet.write_string(2, 1, "ID").unwrap();
    sheet.write_string(3, 1, "0012").unwrap();
    sheet.write_string(99, 3, "outside").unwrap();
    book.save(&path).unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_octa-xlsx"))
            .arg(&path)
            .args(args)
            .output()
            .unwrap()
    };
    let page = run(&["2", "Leads"]);
    assert!(page.status.success());
    assert_eq!(page.stdout, b",ID\n,0012\n,\n");
    assert_eq!(run(&["0"]).stdout, b"First\n");
    assert!(!run(&["1", "Absent"]).status.success());
}
