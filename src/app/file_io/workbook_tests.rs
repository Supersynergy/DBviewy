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
