use super::*;
use rust_xlsxwriter::{Color, Format, Formula, Workbook};

fn fixture() -> (tempfile::TempDir, std::path::PathBuf, DataTable) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("two.xlsx");
    let mut book = Workbook::new();
    book.add_worksheet()
        .set_name("Overview")
        .unwrap()
        .write_string(0, 0, "Unchanged")
        .unwrap();
    let sheet = book.add_worksheet().set_name("Costs & sales").unwrap();
    sheet.write_string(2, 3, "Amount").unwrap();
    sheet.write_string(2, 4, "Double").unwrap();
    sheet
        .write_number_with_format(
            3,
            3,
            12.0,
            &Format::new()
                .set_bold()
                .set_background_color(Color::RGB(0x99cc99)),
        )
        .unwrap();
    sheet
        .write_formula(3, 4, Formula::new("=D4*2").set_result("24"))
        .unwrap();
    sheet.write_number(4, 3, 700).unwrap();
    sheet.autofilter(2, 3, 4, 4).unwrap();
    book.save(&path).unwrap();
    let table = ExcelReader.read_table(&path, "Costs & sales").unwrap();
    (dir, path, table)
}

fn members(path: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    let mut archive = zip::ZipArchive::new(File::open(path).unwrap()).unwrap();
    (0..archive.len())
        .map(|i| {
            let mut entry = archive.by_index(i).unwrap();
            let name = entry.name().unwrap().into_owned();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            (name, bytes)
        })
        .collect()
}

#[test]
fn preserves_other_parts_formulas_and_cell_styles() {
    let (_dir, path, mut table) = fixture();
    let mut before = members(&path);
    table
        .edits
        .insert((0, 0), CellValue::String(" <&> ".into()));
    save(&path, Some("Costs & sales"), &table).unwrap();
    let back = ExcelReader.read_table(&path, "Costs & sales").unwrap();
    assert_eq!(back.rows[0][0], CellValue::String(" <&> ".into()));
    assert_eq!(back.formulas, table.formulas);
    let mut after = members(&path);
    let part = "xl/worksheets/sheet2.xml";
    let previous = String::from_utf8(before.remove(part).unwrap()).unwrap();
    let next = String::from_utf8(after.remove(part).unwrap()).unwrap();
    assert!(previous.contains(r#"<autoFilter ref="D3:E5""#));
    let replaced = previous.replace("<c r=\"D4\" s=\"1\"><v>12</v></c>", "<c r=\"D4\" s=\"1\" t=\"inlineStr\"><is><t xml:space=\"preserve\"> &lt;&amp;&gt; </t></is></c>");
    assert_ne!(previous, replaced);
    assert_eq!(next, replaced);
    assert_eq!(after, before);
}

#[test]
fn unsafe_saves_keep_original_bytes_and_edits() {
    let (_dir, path, mut table) = fixture();
    let original = std::fs::read(&path).unwrap();
    table.edits.insert((0, 1), CellValue::Int(50));
    assert!(save(&path, Some("Costs & sales"), &table).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert_eq!(table.edits.len(), 1);
    table.structural_changes = true;
    assert!(save(&path, Some("Costs & sales"), &table).is_err());
    assert_eq!(std::fs::read(path).unwrap(), original);
}

#[test]
fn fill_originally_absent_cell_through_save_and_reopen() {
    use crate::app::state::OctaApp;
    use octa::ui::settings::{AppSettings, IconVariant};
    let (_dir, path, baseline) = fixture();
    let mut before = members(&path);
    let part = "xl/worksheets/sheet2.xml";
    let original = String::from_utf8(before.remove(part).unwrap()).unwrap();
    assert!(!original.contains(r#"r="E5""#));

    let mut app = OctaApp::new(vec![], AppSettings::default(), IconVariant::Rose);
    app.load_file(path.clone());
    let tab = app
        .tabs
        .iter()
        .position(|tab| tab.sheet_name.as_deref() == Some("Costs & sales"))
        .unwrap();
    assert_eq!(app.tabs[tab].table.get(1, 1), Some(&CellValue::Null));
    app.tabs[tab].table.set(1, 1, CellValue::Bool(true));
    app.save_tab(tab);
    assert!(
        !app.tabs[tab].table.is_modified(),
        "{:?}",
        app.status_message
    );
    let reopened = ExcelReader.read_table(&path, "Costs & sales").unwrap();
    assert_eq!(reopened.get(1, 1), Some(&CellValue::Bool(true)));
    assert_eq!(reopened.formulas, baseline.formulas);

    let expected = original.replace(
        r#"<c r="D5"><v>700</v></c>"#,
        r#"<c r="D5"><v>700</v></c><c r="E5" t="b"><v>1</v></c>"#,
    );
    assert_ne!(expected, original);
    let mut after = members(&path);
    assert_eq!(after.remove(part).unwrap(), expected.as_bytes());
    assert_eq!(after, before);
}

#[test]
fn inserts_sparse_cells_in_numeric_column_order() {
    let xml = r#"<worksheet><sheetData><row r="7" ht="16">
<c r="Z7" s="2"><v>5</v></c><c r="AC7"><f>Z7*2</f><v>10</v></c>
</row></sheetData><mergeCells><mergeCell ref="AE7:AF7"/></mergeCells></worksheet>"#;
    let value = CellValue::Int(4);
    let text = CellValue::String("<&>".into());
    let changes = [
        ("AD7", &text),
        ("AB7", &value),
        ("Z7", &value),
        ("AA7", &value),
        ("Y7", &value),
    ];
    assert_eq!(
        patch(
            xml,
            changes
                .into_iter()
                .map(|(r, v)| (r.to_owned(), v))
                .collect(),
            &PatchTypes::default(),
        )
        .unwrap(),
        r#"<worksheet><sheetData><row r="7" ht="16">
<c r="Y7" t="n"><v>4</v></c><c r="Z7" s="2" t="n"><v>4</v></c><c r="AA7" t="n"><v>4</v></c><c r="AB7" t="n"><v>4</v></c><c r="AC7"><f>Z7*2</f><v>10</v></c>
<c r="AD7" t="inlineStr"><is><t xml:space="preserve">&lt;&amp;&gt;</t></is></c></row></sheetData><mergeCells><mergeCell ref="AE7:AF7"/></mergeCells></worksheet>"#
    );
}

#[test]
fn refuses_to_guess_sparse_row_layout() {
    let value = CellValue::Int(4);
    for (data, error) in [
        (r#"<row><c r="A2"/></row>"#, "unnumbered rows"),
        (r#"<row r="2"><c/></row>"#, "unnumbered cells"),
        (
            r#"<row r="2"><c r="A3"/></row>"#,
            "Ambiguous worksheet cells",
        ),
        (
            r#"<row r="2"><c r="D2"/><c r="A2"/></row>"#,
            "Ambiguous worksheet cells",
        ),
        (
            r#"<row r="2"><c r="A2"/></row><row r="2"><c r="D2"/></row>"#,
            "Ambiguous worksheet row",
        ),
        (r#"<row r="2"><extLst/></row>"#, "unsupported metadata"),
        (
            r#"<row r="2" spans="3:4"><c r="D2"/></row>"#,
            "outside the worksheet's row span",
        ),
        (r#"<row r="2"/>"#, "existing worksheet rows"),
    ] {
        let xml = format!("<worksheet><sheetData>{data}</sheetData></worksheet>");
        let err = patch(
            &xml,
            HashMap::from([("B2".into(), &value)]),
            &PatchTypes::default(),
        )
        .unwrap_err();
        assert!(err.to_string().contains(error), "{data}: {err:#}");
    }
}

#[test]
fn rejects_missing_cells_in_merged_and_formula_ranges() {
    let value = CellValue::Int(4);
    for (data, extras, error) in [
        (
            r#"<c r="A2"/>"#,
            r#"<mergeCells><mergeCell ref="A2:B3"/></mergeCells>"#,
            "merged range A2:B3",
        ),
        (
            r#"<c r="A2"/>"#,
            r#"<mergeCells><mergeCell ref="A2:XFE3"/></mergeCells>"#,
            "Unrecognised merged range",
        ),
        (
            r#"<c r="A2"><f t="array" ref="A2:B3">1+1</f><v>2</v></c>"#,
            "",
            "Range formulas",
        ),
        (r#"<c r="A2"/>"#, "<sheetProtection/>", "protected"),
    ] {
        let xml = format!(
            r#"<worksheet><sheetData><row r="2">{data}</row></sheetData>{extras}</worksheet>"#
        );
        let err = patch(
            &xml,
            HashMap::from([("B2".into(), &value)]),
            &PatchTypes::default(),
        )
        .unwrap_err();
        assert!(err.to_string().contains(error), "{xml}: {err:#}");
    }
}

#[test]
fn missing_row_rejects_entire_workbook_edit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("missing-row.xlsx");
    let mut book = Workbook::new();
    let sheet = book.add_worksheet();
    sheet.write_string(0, 0, "Total").unwrap();
    sheet.write_number(1, 0, 2).unwrap();
    sheet.write_number(3, 0, 4).unwrap();
    book.save(&path).unwrap();
    let mut table = ExcelReader.read_table(&path, "Sheet1").unwrap();
    assert_eq!(table.get(1, 0), Some(&CellValue::Null));
    let original = std::fs::read(&path).unwrap();
    table.set(0, 0, CellValue::Int(20));
    table.set(1, 0, CellValue::Int(30));
    let err = save(&path, Some("Sheet1"), &table).unwrap_err();
    assert!(format!("{err:#}").contains("existing worksheet rows"));
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert_eq!(table.edits.len(), 2);
    assert_eq!(table.get(1, 0), Some(&CellValue::Int(30)));
}

#[test]
fn recalc_settings_preserve_order_and_manual_mode() {
    let disabled =
        r#"<workbook><sheets/><calcPr calcId="42" fullCalcOnLoad="0" iterate="1"/></workbook>"#;
    let requested = request_recalculation(disabled).unwrap();
    assert!(requested.contains(r#"<calcPr calcId="42" iterate="1" fullCalcOnLoad="1"/>"#));
    let missing = "<workbook><sheets/><extLst/></workbook>";
    assert_eq!(
        request_recalculation(missing).unwrap(),
        r#"<workbook><sheets/><calcPr fullCalcOnLoad="1"/><extLst/></workbook>"#
    );
    assert!(request_recalculation(r#"<workbook><calcPr calcMode="manual"/></workbook>"#).is_err());
}

#[test]
fn ods_original_requires_a_separate_export() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("original.ODS");
    let (_, _, source) = fixture();
    let reader = octa::formats::ods_reader::OdsReader;
    reader.write_file(&path, &source).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let table = reader.read_file(&path).unwrap();
    assert!(original(&table, &path));
    assert!(!original(&table, &dir.path().join("export.ods")));
    assert!(
        save(&path, None, &table)
            .unwrap_err()
            .to_string()
            .contains("Save As")
    );
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let export = dir.path().join("export.ods");
    reader.write_file(&export, &table).unwrap();
    assert_eq!(reader.read_file(&export).unwrap().rows, table.rows);
}

#[test]
fn retry_after_backing_file_failure_keeps_plain_copy_and_accepts_another_edit() {
    let (_dir, path, mut table) = fixture();
    table.edits.insert((0, 0), CellValue::Int(15));
    save(&path, Some("Costs & sales"), &table).unwrap();
    table.edits = table.apply_edits_recoverable();
    let disk = std::fs::read(&path).unwrap();
    let stamp = super::super::file_stamp(&path);
    save(&path, Some("Costs & sales"), &table).unwrap();
    assert_eq!(super::super::file_stamp(&path), stamp);
    assert_eq!(std::fs::read(&path).unwrap(), disk);
    table.apply_edits();
    table.edits.insert((0, 0), CellValue::Int(17));
    save(&path, Some("Costs & sales"), &table).unwrap();
    let read = ExcelReader.read_table(&path, "Costs & sales").unwrap();
    assert!(same_value(&read.rows[0][0], &CellValue::Int(17)));
}

#[cfg(target_os = "macos")]
#[test]
fn atomic_replacement_preserves_finder_tags_and_custom_xattrs() {
    let (_dir, path, mut table) = fixture();
    let attrs = [
        (
            "com.apple.metadata:_kMDItemUserTags",
            "62706c6973743030a101564f6374610a36080a0000000000000101000000000000000200000000000000000000000000000011",
        ),
        ("org.octa.test", "0001ff"),
    ];
    for (name, hex) in attrs {
        assert!(
            std::process::Command::new("/usr/bin/xattr")
                .args(["-wx", name, hex])
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
    }
    table.edits.insert((0, 0), CellValue::Int(19));
    save(&path, Some("Costs & sales"), &table).unwrap();
    for (name, hex) in attrs {
        let back = std::process::Command::new("/usr/bin/xattr")
            .args(["-px", name])
            .arg(&path)
            .output()
            .unwrap();
        assert!(back.status.success());
        let value = String::from_utf8(back.stdout)
            .unwrap()
            .split_whitespace()
            .collect::<String>();
        assert_eq!(value.to_ascii_lowercase(), hex);
    }
}

#[test]
fn edited_input_reenables_recalculation_without_touching_cached_formulas() {
    let (_dir, path, mut table) = fixture();
    let mut parts = members(&path);
    let workbook = parts.get_mut("xl/workbook.xml").unwrap();
    let before = String::from_utf8(workbook.clone()).unwrap();
    let changed = before.replace(r#"fullCalcOnLoad="1""#, r#"fullCalcOnLoad="0""#);
    assert_ne!(before, changed);
    *workbook = changed.into_bytes();
    let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
    for (name, payload) in &parts {
        zip.start_file(name.as_str(), zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(payload).unwrap();
    }
    zip.finish().unwrap();
    table.edits.insert((0, 0), CellValue::Int(34));
    save(&path, Some("Costs & sales"), &table).unwrap();
    let after = members(&path);
    assert_eq!(
        std::str::from_utf8(&after["xl/workbook.xml"]).unwrap(),
        before
    );
    let reopened = ExcelReader.read_table(&path, "Costs & sales").unwrap();
    assert_eq!(reopened.formulas, table.formulas);
    assert!(same_value(&reopened.rows[0][0], &CellValue::Int(34)));
    assert!(same_value(&reopened.rows[0][1], &CellValue::Int(24)));
}

#[test]
fn gui_save_as_copies_all_sheets_and_continues_editing_the_named_sheet() {
    use crate::app::state::OctaApp;
    use octa::ui::settings::{AppSettings, IconVariant};
    let (dir, source, _) = fixture();
    let original = std::fs::read(&source).unwrap();
    let mut app = OctaApp::new(vec![], AppSettings::default(), IconVariant::Rose);
    app.load_file(source.clone());
    let idx = app
        .tabs
        .iter()
        .position(|tab| tab.sheet_name.as_deref() == Some("Costs & sales"))
        .unwrap();

    // A copy with no edits also creates a complete workbook.
    let unchanged = dir.path().join("unchanged.xlsx");
    app.finish_save_as(idx, unchanged.clone(), false, false, None);
    assert_eq!(std::fs::read(unchanged).unwrap(), original);

    app.tabs[idx].table.set(0, 0, CellValue::Int(73));
    let copy = dir.path().join("revised.xlsx");
    app.finish_save_as(idx, copy.clone(), false, true, None);
    assert_eq!(app.tabs[idx].sheet_name.as_deref(), Some("Costs & sales"));
    assert_eq!(
        app.tabs[idx].table.source_path.as_deref().map(Path::new),
        Some(copy.as_path())
    );
    let mut src_parts = members(&source);
    let mut dst_parts = members(&copy);
    let active = "xl/worksheets/sheet2.xml";
    let source_xml = String::from_utf8(src_parts.remove(active).unwrap()).unwrap();
    let copy_xml = String::from_utf8(dst_parts.remove(active).unwrap()).unwrap();
    assert_eq!(
        copy_xml,
        source_xml.replace(
            r#"<c r="D4" s="1"><v>12</v></c>"#,
            r#"<c r="D4" s="1" t="n"><v>73</v></c>"#
        )
    );
    assert_eq!(src_parts, dst_parts);
    app.tabs[idx].table.set(0, 0, CellValue::Int(80));
    app.save_tab(idx);
    let saved = ExcelReader.read_table(&copy, "Costs & sales").unwrap();
    assert!(same_value(&saved.rows[0][0], &CellValue::Int(80)));
    assert_eq!(std::fs::read(source).unwrap(), original);
}

#[test]
fn gui_save_as_rejected_edit_leaves_existing_target_and_new_name_untouched() {
    use crate::app::state::OctaApp;
    use octa::ui::settings::{AppSettings, IconVariant};
    let (dir, source, _) = fixture();
    let original = std::fs::read(&source).unwrap();
    let mut app = OctaApp::new(vec![], AppSettings::default(), IconVariant::Rose);
    app.load_file(source.clone());
    let idx = app
        .tabs
        .iter()
        .position(|t| t.sheet_name.as_deref() == Some("Costs & sales"))
        .unwrap();
    app.tabs[idx].table.set(0, 1, CellValue::Int(7)); // Formula cell
    let existing = dir.path().join("previous.xlsx");
    std::fs::write(&existing, b"previous destination").unwrap();
    app.finish_save_as(idx, existing.clone(), false, true, None);
    assert_eq!(std::fs::read(existing).unwrap(), b"previous destination");
    let new = dir.path().join("new.xlsx");
    app.finish_save_as(idx, new.clone(), false, false, None);
    assert!(!new.exists());
    assert_eq!(
        app.tabs[idx].table.source_path.as_deref().map(Path::new),
        Some(source.as_path())
    );
    assert!(app.tabs[idx].table.is_modified());
    assert_eq!(std::fs::read(&source).unwrap(), original);

    app.settings.auto_save_enabled = true;
    app.settings.auto_save_interval_minutes = 1;
    app.last_auto_save = std::time::Instant::now() - std::time::Duration::from_secs(61);
    app.drive_auto_save(&eframe::egui::Context::default());
    assert!(app.tabs[idx].table.is_modified());
    assert!(
        app.status_message
            .as_ref()
            .unwrap()
            .0
            .contains("Formula cells")
    );
    assert_eq!(std::fs::read(&source).unwrap(), original);
}

#[cfg(unix)]
#[test]
fn saving_relative_symlink_updates_its_target_and_rejects_conflicting_links() {
    use std::os::unix::fs::symlink;
    let (dir, target, _) = fixture();
    let links = dir.path().join("links");
    std::fs::create_dir(&links).unwrap();
    let relative = Path::new("../two.xlsx");
    let link = links.join("working.xlsx");
    symlink(relative, &link).unwrap();
    let mut table = ExcelReader.read_table(&link, "Costs & sales").unwrap();
    table.set(0, 0, CellValue::Int(91));
    save(&link, Some("Costs & sales"), &table).unwrap();
    assert_eq!(std::fs::read_link(&link).unwrap(), relative);
    let reopened = ExcelReader.read_table(&target, "Costs & sales").unwrap();
    assert!(same_value(&reopened.rows[0][0], &CellValue::Int(91)));

    let destination = links.join("export.xlsx");
    let other = dir.path().join("other.xlsx");
    std::fs::copy(&target, &other).unwrap();
    symlink("../other.xlsx", &destination).unwrap();
    let bytes = std::fs::read(&other).unwrap();
    table.apply_edits();
    table.set(0, 0, CellValue::Int(12));
    assert!(save(&destination, Some("Costs & sales"), &table).is_err());
    assert_eq!(std::fs::read(&other).unwrap(), bytes);
    assert_eq!(
        std::fs::read_link(&destination).unwrap(),
        Path::new("../other.xlsx")
    );
    std::fs::remove_file(other).unwrap();
    assert!(save(&destination, Some("Costs & sales"), &table).is_err());
    assert_eq!(
        std::fs::read_link(&destination).unwrap(),
        Path::new("../other.xlsx")
    );
}

#[test]
fn filtered_edit_saves_the_underlying_row_and_filtered_export_stays_a_table() {
    use crate::app::state::OctaApp;
    use octa::ui::settings::{AppSettings, IconVariant};

    let (dir, source, _) = fixture();
    let mut app = OctaApp::new(vec![], AppSettings::default(), IconVariant::Rose);
    app.load_file(source.clone());
    let idx = app
        .tabs
        .iter()
        .position(|t| t.sheet_name.as_deref() == Some("Costs & sales"))
        .unwrap();
    app.active_tab = idx;
    app.tabs[idx].search_text = "700".into();
    app.recompute_filter();
    assert_eq!(app.tabs[idx].filtered_rows, [1]);
    let row = app.tabs[idx].filtered_rows[0];
    app.tabs[idx].table.set(row, 0, CellValue::Int(1700));
    app.save_tab(idx);

    let result = ExcelReader.read_table(&source, "Costs & sales").unwrap();
    assert!(same_value(&result.rows[0][0], &CellValue::Int(12)));
    assert!(same_value(&result.rows[1][0], &CellValue::Int(1700)));
    assert!(result.formula(0, 1).is_some());
    assert_eq!(open_workbook_auto(&source).unwrap().sheet_names().len(), 2);

    let export = dir.path().join("filtered.xlsx");
    app.finish_save_as(idx, export.clone(), true, false, None);
    assert_eq!(open_workbook_auto(&export).unwrap().sheet_names().len(), 1);
    let exported = ExcelReader.read_file(&export).unwrap();
    assert_eq!(exported.row_count(), 1);
    assert!(same_value(&exported.rows[0][0], &CellValue::Int(1700)));
}

#[test]
fn clear_existing_literal_keeps_style_and_all_other_workbook_parts() {
    let (_dir, path, mut table) = fixture();
    let part = "xl/worksheets/sheet2.xml";
    let mut before = members(&path);
    table.set(0, 0, CellValue::Null);
    save(&path, Some("Costs & sales"), &table).unwrap();
    let mut after = members(&path);
    let old = String::from_utf8(before.remove(part).unwrap()).unwrap();
    let cleared = String::from_utf8(after.remove(part).unwrap()).unwrap();
    let expected = old.replace(r#"<c r="D4" s="1"><v>12</v></c>"#, r#"<c r="D4" s="1"/>"#);
    assert_ne!(old, expected);
    assert_eq!(cleared, expected);
    assert_eq!(before, after);
    let mut reloaded = ExcelReader.read_table(&path, "Costs & sales").unwrap();
    assert_eq!(reloaded.get(0, 0), Some(&CellValue::Null));
    assert_eq!(reloaded.formulas, table.formulas);
    assert!(same_value(reloaded.get(0, 1).unwrap(), &CellValue::Int(24)));

    reloaded.set(0, 0, CellValue::Int(33));
    save(&path, Some("Costs & sales"), &reloaded).unwrap();
    let parts = members(&path);
    assert!(
        std::str::from_utf8(&parts[part])
            .unwrap()
            .contains(r#"<c r="D4" s="1" t="n"><v>33</v></c>"#)
    );
    let reloaded = ExcelReader.read_table(&path, "Costs & sales").unwrap();
    assert!(same_value(reloaded.get(0, 0).unwrap(), &CellValue::Int(33)));
}

#[test]
fn clear_last_row_through_cut_then_refill_in_the_same_tab() {
    use crate::app::state::OctaApp;
    use octa::ui::settings::{AppSettings, IconVariant};
    let (_dir, path, _) = fixture();
    let mut app = OctaApp::new(vec![], AppSettings::default(), IconVariant::Rose);
    app.load_file(path.clone());
    let tab = app
        .tabs
        .iter()
        .position(|tab| tab.sheet_name.as_deref() == Some("Costs & sales"))
        .unwrap();
    app.active_tab = tab;
    app.tabs[tab].table_state.selected_cell = Some((1, 0));
    // The context retains the clipboard output locally; it doesn't reach
    // an OS clipboard when there is no windowing integration.
    let ctx = eframe::egui::Context::default();
    app.do_cut(&ctx);
    assert_eq!(app.tabs[tab].table.get(1, 0), Some(&CellValue::Null));
    app.save_tab(tab);
    assert!(
        !app.tabs[tab].table.is_modified(),
        "{:?}",
        app.status_message
    );
    let cleared = ExcelReader.read_table(&path, "Costs & sales").unwrap();
    assert_eq!(cleared.row_count(), 1, "the reader trims the blank row");
    assert_eq!(
        app.tabs[tab].table.row_count(),
        2,
        "the open tab retains its layout"
    );
    let parts = members(&path);
    assert!(
        std::str::from_utf8(&parts["xl/worksheets/sheet2.xml"])
            .unwrap()
            .contains(r#"<c r="D5"/>"#)
    );

    app.tabs[tab].table.set(1, 0, CellValue::Int(19));
    app.save_tab(tab);
    assert!(
        !app.tabs[tab].table.is_modified(),
        "{:?}",
        app.status_message
    );
    let reloaded = ExcelReader.read_table(&path, "Costs & sales").unwrap();
    assert_eq!(reloaded.row_count(), 2);
    assert!(same_value(reloaded.get(1, 0).unwrap(), &CellValue::Int(19)));
    assert_eq!(reloaded.formulas, cleared.formulas);
}

#[test]
fn clear_shared_string_and_boolean_keeps_archive_references_intact() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("scalars.xlsx");
    let mut book = Workbook::new();
    let sheet = book.add_worksheet().set_name("Scalars").unwrap();
    for (col, name) in ["label", "done", "amount"].into_iter().enumerate() {
        sheet.write_string(0, col as u16, name).unwrap();
    }
    sheet
        .write_string(1, 0, "Label that was stored in shared strings")
        .unwrap();
    sheet.write_boolean(1, 1, true).unwrap();
    sheet.write_number(1, 2, 777).unwrap();
    book.save(&path).unwrap();

    let mut before = members(&path);
    let part = "xl/worksheets/sheet1.xml";
    let old = String::from_utf8(before.remove(part).unwrap()).unwrap();
    let mut table = ExcelReader.read_table(&path, "Scalars").unwrap();
    table.set(0, 0, CellValue::Null);
    table.set(0, 1, CellValue::Null);
    save(&path, Some("Scalars"), &table).unwrap();
    let mut after = members(&path);
    let new = String::from_utf8(after.remove(part).unwrap()).unwrap();
    let expected = old
        .replace(r#"<c r="A2" t="s"><v>3</v></c>"#, r#"<c r="A2"/>"#)
        .replace(r#"<c r="B2" t="b"><v>1</v></c>"#, r#"<c r="B2"/>"#);
    assert_ne!(expected, old);
    assert_eq!(new, expected);
    assert_eq!(after, before);
    let reloaded = ExcelReader.read_table(&path, "Scalars").unwrap();
    assert_eq!(reloaded.get(0, 0), Some(&CellValue::Null));
    assert_eq!(reloaded.get(0, 1), Some(&CellValue::Null));
    assert!(same_value(
        reloaded.get(0, 2).unwrap(),
        &CellValue::Int(777)
    ));
}

#[test]
fn clear_formula_rejects_entire_batch_without_writing_any_cell() {
    let (_dir, path, mut table) = fixture();
    let before = std::fs::read(&path).unwrap();
    table.set(0, 0, CellValue::Null);
    table.set(0, 1, CellValue::Null);
    let error = save(&path, Some("Costs & sales"), &table).unwrap_err();
    assert!(error.to_string().contains("Formula cells"), "{error:#}");
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(table.edits.get(&(0, 0)), Some(&CellValue::Null));
    assert_eq!(table.edits.get(&(0, 1)), Some(&CellValue::Null));
}

#[test]
fn stale_nonblank_trailing_row_is_not_accepted_after_another_edit() {
    let (_dir, path, mut old) = fixture();
    let mut other = ExcelReader.read_table(&path, "Costs & sales").unwrap();
    other.set(1, 0, CellValue::Null);
    save(&path, Some("Costs & sales"), &other).unwrap();
    let externally_saved = std::fs::read(&path).unwrap();

    old.set(0, 0, CellValue::Int(66));
    let error = save(&path, Some("Costs & sales"), &old).unwrap_err();
    assert!(error.to_string().contains("Reopen it"), "{error:#}");
    assert_eq!(std::fs::read(&path).unwrap(), externally_saved);
    assert_eq!(old.edits.get(&(0, 0)), Some(&CellValue::Int(66)));
}

fn calendar_fixture() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("schedule.xlsx");
    let mut workbook = Workbook::new();
    workbook
        .add_worksheet()
        .set_name("Overview")
        .unwrap()
        .write_string(0, 0, "Keep this sheet")
        .unwrap();
    let sheet = workbook
        .add_worksheet()
        .set_name("Invoice schedule")
        .unwrap();
    let format = Format::new()
        .set_num_format("yyyy-mm-dd hh:mm:ss.000")
        .set_bold();
    for (c, header) in ["Due date", "Amount", "Day"].into_iter().enumerate() {
        sheet.write_string(0, c as u16, header).unwrap();
    }
    for (row, serial) in [(1, 45_361.5), (2, 45_362.0)] {
        sheet
            .write_number_with_format(row, 0, serial, &format)
            .unwrap();
        sheet.write_number(row, 1, 70).unwrap();
    }
    sheet
        .write_formula(1, 2, Formula::new("=DAY(A2)").set_result("10"))
        .unwrap();
    sheet.autofilter(0, 0, 2, 2).unwrap();
    workbook.save(&path).unwrap();
    (dir, path)
}

#[test]
fn existing_datetime_keeps_the_workbook_when_saving_again_from_the_same_tab() {
    use crate::app::state::OctaApp;
    use octa::data::BinaryDisplayMode;
    use octa::ui::{
        settings::{AppSettings, IconVariant},
        table_view::commit_cell_text,
    };
    let (_dir, path) = calendar_fixture();
    let mut before = members(&path);
    let mut app = OctaApp::new(vec![], AppSettings::default(), IconVariant::Rose);
    app.load_file(path.clone());
    let idx = app
        .tabs
        .iter()
        .position(|tab| tab.sheet_name.as_deref() == Some("Invoice schedule"))
        .unwrap();
    assert!(commit_cell_text(
        &mut app.tabs[idx].table,
        0,
        0,
        "2024-03-15T18:45:03.125".into(),
        BinaryDisplayMode::default()
    ));
    app.save_tab(idx);
    assert!(
        !app.tabs[idx].table.is_modified(),
        "{:?}",
        app.status_message
    );
    let after = ExcelReader.read_table(&path, "Invoice schedule").unwrap();
    assert_eq!(
        after.get(0, 0),
        Some(&CellValue::DateTime("2024-03-15 18:45:03.125".into()))
    );
    assert_eq!(after.formulas, app.tabs[idx].table.formulas);
    assert!(commit_cell_text(
        &mut app.tabs[idx].table,
        1,
        0,
        "2024-03-18".into(),
        BinaryDisplayMode::default()
    ));
    app.save_tab(idx);
    assert!(
        !app.tabs[idx].table.is_modified(),
        "{:?}",
        app.status_message
    );
    let saved = ExcelReader.read_table(&path, "Invoice schedule").unwrap();
    assert_eq!(
        saved.get(1, 0),
        Some(&CellValue::DateTime("2024-03-18 00:00:00".into()))
    );
    assert_eq!(saved.get(0, 0), after.get(0, 0));

    let mut after = members(&path);
    let member = "xl/worksheets/sheet2.xml";
    let initial = String::from_utf8(before.remove(member).unwrap()).unwrap();
    let edited = String::from_utf8(after.remove(member).unwrap()).unwrap();
    let old = r#"<c r="A2" s="1"><v>45361.5</v></c>"#;
    let start = edited.find(r#"<c r="A2" s="1" t="n"><v>"#).unwrap();
    let end = start + edited[start..].find("</c>").unwrap() + "</c>".len();
    let first = &edited[start..end];
    assert!(initial.contains(old));
    assert_eq!(
        edited,
        initial.replace(old, first).replace(
            r#"<c r="A3" s="1"><v>45362</v></c>"#,
            r#"<c r="A3" s="1" t="n"><v>45369</v></c>"#,
        )
    );
    assert_eq!(after, before);
}

#[test]
fn legacy_1904_date_edits_preserve_the_epoch_and_reject_unrepresentable_dates() {
    let (_dir, path) = calendar_fixture();
    let parts = members(&path);
    let mut book = zip::ZipWriter::new(File::create(&path).unwrap());
    for (name, bytes) in &parts {
        book.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        if name == "xl/workbook.xml" {
            let xml = std::str::from_utf8(bytes).unwrap();
            assert_eq!(xml.matches("<workbookPr ").count(), 1);
            book.write_all(
                xml.replace(
                    "<workbookPr ",
                    r#"<ss:workbookPr xmlns:ss="http://schemas.openxmlformats.org/spreadsheetml/2006/main" date1904="1" "#,
                )
                    .as_bytes(),
            )
            .unwrap();
        } else {
            book.write_all(bytes).unwrap();
        }
    }
    book.finish().unwrap();
    let mut table = ExcelReader.read_table(&path, "Invoice schedule").unwrap();
    table.set(0, 0, CellValue::DateTime("2011-07-05 06:00:00".into()));
    save(&path, Some("Invoice schedule"), &table).unwrap();
    let parts = members(&path);
    let edited = std::str::from_utf8(&parts["xl/worksheets/sheet2.xml"]).unwrap();
    assert!(edited.contains(r#"<c r="A2" s="1" t="n"><v>39267.25</v></c>"#));
    assert!(
        std::str::from_utf8(&parts["xl/workbook.xml"])
            .unwrap()
            .contains("date1904=\"1\"")
    );
    let mut reopened = ExcelReader.read_table(&path, "Invoice schedule").unwrap();
    assert_eq!(
        reopened.get(0, 0),
        Some(&CellValue::DateTime("2011-07-05 06:00:00".into()))
    );
    let saved = std::fs::read(&path).unwrap();
    reopened.set(1, 0, CellValue::Date("1904-01-01".into()));
    let error = save(&path, Some("Invoice schedule"), &reopened).unwrap_err();
    assert!(format!("{error:#}").contains("outside the workbook's calendar"));
    assert_eq!(std::fs::read(&path).unwrap(), saved);
}

#[test]
fn ambiguous_workbook_date_system_is_not_guessed() {
    let valid =
        r#"<workbook><workbookPr date1904="true"/><extLst><workbookPr/></extLst></workbook>"#;
    assert!(dates::uses_1904_epoch(valid).unwrap());
    for (xml, message) in [
        (
            r#"<workbook><workbookPr date1904="false"/><workbookPr date1904="true"/></workbook>"#,
            "Ambiguous",
        ),
        (
            r#"<workbook><extLst><workbookPr date1904="true"/></extLst></workbook>"#,
            "Nested",
        ),
        (
            r#"<workbook><workbookPr date1904="&#49;"/></workbook>"#,
            "Unrecognised",
        ),
    ] {
        let error = dates::uses_1904_epoch(xml).unwrap_err();
        assert!(format!("{error:#}").contains(message));
    }
}

#[test]
fn bad_date_changes_roll_back_the_entire_xlsx_edit() {
    let (_dir, path) = calendar_fixture();
    let before = std::fs::read(&path).unwrap();
    let base = ExcelReader.read_table(&path, "Invoice schedule").unwrap();
    for (date, error) in [
        ("1900-02-29", "Use YYYY-MM-DD"),
        ("2024-03-31T23:59:59.999999999", "millisecond precision"),
        ("2024-03-31T23:59:59+02:00", "without an offset"),
    ] {
        let mut table = base.clone();
        table.set(0, 1, CellValue::Int(500));
        table.set(0, 0, CellValue::DateTime(date.into()));
        let result = save(&path, Some("Invoice schedule"), &table).unwrap_err();
        assert!(format!("{result:#}").contains(error), "{result:#}");
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(table.edits.len(), 2);
    }
    let mut table = base;
    table.set(0, 1, CellValue::Date("2024-03-31".into()));
    let result = save(&path, Some("Invoice schedule"), &table).unwrap_err();
    assert!(format!("{result:#}").contains("New date formats"));
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn explicit_iso_date_type_is_kept_without_adding_a_style() {
    let sheet = r#"<worksheet><sheetData><row r="2"><c r="A2" t="d"><v>2024-03-11T08:00:00</v></c></row></sheetData></worksheet>"#;
    let next = CellValue::DateTime("2024-03-12 09:30".into());
    let updated = patch(
        sheet,
        HashMap::from([("A2".into(), &next)]),
        &PatchTypes {
            date_1904: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        updated,
        sheet.replace("2024-03-11T08:00:00", "2024-03-12T09:30:00")
    );
}

fn timed_workbook(date_1904: bool) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shift-summary.xlsx");
    let mut book = Workbook::new();
    book.add_worksheet()
        .set_name("Overview")
        .unwrap()
        .write_string(0, 0, "Hours by shift")
        .unwrap();
    let sheet = book.add_worksheet().set_name("Shifts").unwrap();
    for (c, heading) in [
        "Start",
        "Elapsed",
        "Mixed",
        "Billed hours",
        "Calculated start",
    ]
    .into_iter()
    .enumerate()
    {
        sheet.write_string(2, c as u16 + 3, heading).unwrap();
    }
    let clock = Format::new().set_num_format("hh:mm:ss.000").set_bold();
    let elapsed = Format::new().set_num_format("[h]:mm:ss.000").set_italic();
    for (row, day_fraction, duration) in [(3, 0.25, 1.25), (4, 0.75, 2.0)] {
        sheet
            .write_number_with_format(row, 3, day_fraction, &clock)
            .unwrap();
        sheet
            .write_number_with_format(row, 4, duration, &elapsed)
            .unwrap();
        sheet
            .write_formula(
                row,
                6,
                Formula::new(format!("=E{}*24", row + 1)).set_result((duration * 24.0).to_string()),
            )
            .unwrap();
    }
    sheet.write_number_with_format(3, 5, 0.5, &clock).unwrap();
    sheet.write_number_with_format(4, 5, 1.5, &elapsed).unwrap();
    sheet
        .write_formula_with_format(3, 7, Formula::new("=D4").set_result("0.25"), &clock)
        .unwrap();
    sheet.autofilter(2, 3, 4, 7).unwrap();
    book.save(&path).unwrap();
    if date_1904 {
        let mut parts = members(&path);
        let xml = parts.get_mut("xl/workbook.xml").unwrap();
        let text = std::str::from_utf8(xml).unwrap();
        assert_eq!(text.matches("<workbookPr ").count(), 1);
        *xml = text
            .replace("<workbookPr ", "<workbookPr date1904=\"1\" ")
            .into_bytes();
        let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
        for (name, bytes) in &parts {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }
    (dir, path)
}

#[test]
fn save_clock_and_elapsed_corrections_keep_styles_and_other_sheet_parts() {
    use crate::app::state::OctaApp;
    use octa::data::{
        BinaryDisplayMode,
        times::{NANOS_PER_DAY, NANOS_PER_SEC},
    };
    use octa::ui::{
        settings::{AppSettings, IconVariant},
        table_view::commit_cell_text,
    };
    let (_dir, path) = timed_workbook(false);
    let mut original = members(&path);
    let mut app = OctaApp::new(vec![], AppSettings::default(), IconVariant::Rose);
    app.load_file(path.clone());
    let idx = app
        .tabs
        .iter()
        .position(|tab| tab.sheet_name.as_deref() == Some("Shifts"))
        .unwrap();
    let table = &mut app.tabs[idx].table;
    assert_eq!(table.get(0, 0), Some(&CellValue::Time(NANOS_PER_DAY / 4)));
    assert_eq!(
        table.get(0, 1),
        Some(&CellValue::Duration(30 * 3600 * NANOS_PER_SEC))
    );
    for (col, value) in [(0, "23:59:59.999"), (1, "27:30:00.125")] {
        assert!(commit_cell_text(
            table,
            0,
            col,
            value.into(),
            BinaryDisplayMode::default()
        ));
    }
    app.save_tab(idx);
    assert!(
        !app.tabs[idx].table.is_modified(),
        "{:?}",
        app.status_message
    );
    assert!(commit_cell_text(
        &mut app.tabs[idx].table,
        0,
        2,
        "13:15:00".into(),
        BinaryDisplayMode::default()
    ));
    app.save_tab(idx);
    assert!(
        !app.tabs[idx].table.is_modified(),
        "{:?}",
        app.status_message
    );

    let reopened = ExcelReader.read_table(&path, "Shifts").unwrap();
    assert_eq!(
        reopened.get(0, 0),
        Some(&CellValue::Time(NANOS_PER_DAY - 1_000_000))
    );
    assert_eq!(
        reopened.get(0, 1),
        Some(&CellValue::Duration(99_000 * NANOS_PER_SEC + 125_000_000))
    );
    assert_eq!(
        reopened.get(0, 2),
        Some(&CellValue::Duration(47_700 * NANOS_PER_SEC))
    );
    assert_eq!(reopened.formulas, app.tabs[idx].table.formulas);
    let mut edited = members(&path);
    let member = "xl/worksheets/sheet2.xml";
    let before = String::from_utf8(original.remove(member).unwrap()).unwrap();
    let after = String::from_utf8(edited.remove(member).unwrap()).unwrap();
    let mut expected = before;
    for (address, style, previous) in [("D4", 1, "0.25"), ("E4", 2, "1.25"), ("F4", 1, "0.5")] {
        let old = format!(r#"<c r="{address}" s="{style}"><v>{previous}</v></c>"#);
        let new_start = format!(r#"<c r="{address}" s="{style}" t="n"><v>"#);
        let start = after.find(&new_start).unwrap();
        let stop = start + after[start..].find("</c>").unwrap() + "</c>".len();
        assert!(expected.contains(&old), "{old}");
        expected = expected.replace(&old, &after[start..stop]);
    }
    assert_eq!(after, expected);
    assert_eq!(edited, original);
}

#[test]
fn save_negative_elapsed_value_in_existing_1904_workbook_without_epoch_shift() {
    use octa::data::times::{NANOS_PER_DAY, NANOS_PER_SEC};
    let (_dir, path) = timed_workbook(true);
    let mut table = ExcelReader.read_table(&path, "Shifts").unwrap();
    table.set(0, 1, CellValue::Duration(-90 * 60 * NANOS_PER_SEC));
    save(&path, Some("Shifts"), &table).unwrap();
    let reopened = ExcelReader.read_table(&path, "Shifts").unwrap();
    assert_eq!(
        reopened.get(0, 1),
        Some(&CellValue::Duration(-90 * 60 * NANOS_PER_SEC))
    );
    assert_eq!(
        reopened.get(0, 0),
        Some(&CellValue::Time(NANOS_PER_DAY / 4))
    );
    let parts = members(&path);
    let sheet = std::str::from_utf8(&parts["xl/worksheets/sheet2.xml"]).unwrap();
    assert!(sheet.contains(r#"<c r="E4" s="2" t="n"><v>-0.0625</v></c>"#));
    let workbook = std::str::from_utf8(&parts["xl/workbook.xml"]).unwrap();
    assert!(workbook.contains(r#"date1904="1""#));
}

#[test]
fn incompatible_time_edits_leave_the_original_workbook_and_valid_peer_edit_in_place() {
    use octa::data::times::{NANOS_PER_DAY, NANOS_PER_SEC};
    let (_dir, path) = timed_workbook(false);
    let original = std::fs::read(&path).unwrap();
    let base = ExcelReader.read_table(&path, "Shifts").unwrap();
    for (col, value, error) in [
        (0, CellValue::Time(NANOS_PER_DAY), "clock format"),
        (1, CellValue::Duration(-1_000_000), "existing 1904 workbook"),
        (
            1,
            CellValue::Duration(60 * NANOS_PER_SEC + 1),
            "millisecond precision",
        ),
        (
            2,
            CellValue::Duration(NANOS_PER_DAY + NANOS_PER_SEC),
            "clock format",
        ),
        (3, CellValue::Duration(NANOS_PER_SEC), "New time formats"),
        (
            4,
            CellValue::Time(12 * 3600 * NANOS_PER_SEC),
            "Formula cells",
        ),
    ] {
        let mut table = base.clone();
        table.set(1, 1, CellValue::Duration(29 * 3600 * NANOS_PER_SEC));
        table.set(0, col, value);
        let why = save(&path, Some("Shifts"), &table).unwrap_err();
        assert!(format!("{why:#}").contains(error), "{why:#}");
        assert_eq!(table.edits.len(), 2);
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }
}
