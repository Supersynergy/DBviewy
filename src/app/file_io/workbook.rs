//! Patch existing scalar cells without serialising the workbook from its table view.
use anyhow::{Context, Result, bail, ensure};
use calamine::{Reader as _, open_workbook_auto};
use octa::data::{CellValue, DataTable};
use octa::formats::{FormatReader, excel_reader::ExcelReader};
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::path::Path;

pub(super) fn whole_xlsx(
    table: &DataTable,
    target: &Path,
    mode: crate::app::state::SaveMode,
) -> bool {
    let xlsx = |path: &Path| {
        path.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("xlsx"))
    };
    !matches!(
        mode,
        crate::app::state::SaveMode::Copy { filtered: true, .. }
    ) && xlsx(target)
        && table
            .source_path
            .as_deref()
            .map(Path::new)
            .is_some_and(xlsx)
}

pub(super) fn original(table: &DataTable, path: &Path) -> bool {
    let Some(source) = table.source_path.as_deref().map(Path::new) else {
        return false;
    };
    source
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| {
            matches!(
                s.to_ascii_lowercase().as_str(),
                "xlsx" | "xlsm" | "xlsb" | "xls" | "xlm" | "ods"
            )
        })
        && (source == path
            || source
                .canonicalize()
                .ok()
                .is_some_and(|p| path.canonicalize().ok().as_ref() == Some(&p)))
}

fn attr(tag: &BytesStart<'_>, key: &str) -> Result<Option<String>> {
    tag.try_get_attribute(key)?
        .map(|a| {
            Ok(a.normalized_value(quick_xml::XmlVersion::Implicit1_0)?
                .into_owned())
        })
        .transpose()
}

fn lookup(xml: &str, element: &str, key: &str, value: &str, result: &str) -> Result<String> {
    let mut reader = Reader::from_str(xml);
    let mut found = None;
    loop {
        match reader.read_event()? {
            Event::Start(e) | Event::Empty(e)
                if e.name().as_ref() == element && attr(&e, key)?.as_deref() == Some(value) =>
            {
                ensure!(found.is_none(), "Ambiguous workbook relationship");
                found = Some(attr(&e, result)?.context("Missing workbook relationship")?);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    found.context("Could not resolve the worksheet relationship")
}

fn part(zip: &mut zip::ZipArchive<BufReader<&File>>, name: &str) -> Result<String> {
    let limit = octa::formats::compression::max_decompressed();
    let mut value = String::new();
    zip.by_name(name)?
        .take(limit.saturating_add(1))
        .read_to_string(&mut value)?;
    ensure!(
        value.len() as u64 <= limit,
        "Worksheet XML exceeds the configured decompression limit"
    );
    Ok(value)
}

fn cell_value(value: &CellValue) -> Result<(&'static str, String)> {
    Ok(match value {
        CellValue::String(s) => {
            ensure!(
                s.encode_utf16().count() <= 32767
                    && !s.as_bytes().windows(7).any(|w| w.starts_with(b"_x")
                        && w[2..6].iter().all(u8::is_ascii_hexdigit)
                        && w[6] == b'_')
                    && s.chars().all(|c| c == '\n'
                        || c == '\t'
                        || (c >= ' ' && c != '\u{fffe}' && c != '\u{ffff}')),
                "Text cannot be represented safely in this workbook"
            );
            (
                "inlineStr",
                format!(
                    "<is><t xml:space=\"preserve\">{}</t></is>",
                    quick_xml::escape::escape(s.as_str())
                ),
            )
        }
        CellValue::Bool(b) => ("b", format!("<v>{}</v>", u8::from(*b))),
        CellValue::Int(n) if n.unsigned_abs() <= 999_999_999_999_999 => {
            ("n", format!("<v>{n}</v>"))
        }
        CellValue::Float(n) if n.is_finite() => ("n", format!("<v>{n}</v>")),
        _ => bail!("In-place workbook edits support text, numbers and booleans"),
    })
}

// Only plain scalar cell contents can be replaced. Anything else, including rich
// runs, formula elements, extension metadata and custom cell attributes, survives
// by refusing the edit.
fn replace_cell(
    tag: &BytesStart<'_>,
    body: &str,
    address: &str,
    value: &CellValue,
) -> Result<String> {
    for attribute in tag.attributes() {
        ensure!(
            matches!(attribute?.key.as_ref(), "r" | "s" | "t"),
            "Unsupported metadata at {address}"
        );
    }
    let scalar = body
        .strip_prefix("<v>")
        .and_then(|s| s.strip_suffix("</v>"))
        .is_some_and(|s| !s.contains('<'));
    let inline = body
        .strip_prefix("<is><t>")
        .or_else(|| body.strip_prefix("<is><t xml:space=\"preserve\">"))
        .and_then(|s| s.strip_suffix("</t></is>"))
        .is_some_and(|s| !s.contains('<'));
    ensure!(
        body.is_empty() || scalar || inline,
        "Formula or unsupported cell contents at {address}"
    );
    let (kind, contents) = cell_value(value)?;
    let mut out = format!("<c r=\"{address}\"");
    if let Some(style) = attr(tag, "s")? {
        let id: u32 = style.parse()?;
        out.push_str(&format!(" s=\"{id}\""));
    }
    out.push_str(&format!(" t=\"{kind}\">{contents}</c>"));
    Ok(out)
}

fn patch(xml: &str, mut changes: HashMap<String, &CellValue>) -> Result<String> {
    let mut reader = Reader::from_str(xml);
    let mut out = String::with_capacity(xml.len());
    let mut copied = 0;
    let mut stack = Vec::new();
    loop {
        let start = reader.buffer_position() as usize;
        let event = reader.read_event()?;
        match event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let name = e.name();
                ensure!(
                    name.local_name().as_ref() != "sheetProtection",
                    "This worksheet is protected"
                );
                if name.local_name().as_ref() == "f" {
                    ensure!(
                        attr(e, "ref")?.is_none(),
                        "Range formulas require an Excel editor"
                    );
                }
                if name.as_ref() == "c"
                    && stack == ["worksheet", "sheetData", "row"]
                    && let Some(address) = attr(e, "r")?
                    && let Some(value) = changes.remove(&address)
                {
                    let body = if matches!(event, Event::Start(_)) {
                        let span = reader.read_to_end(name)?;
                        &xml[span.start as usize..span.end as usize]
                    } else {
                        ""
                    };
                    out.push_str(&xml[copied..start]);
                    out.push_str(&replace_cell(e, body, &address, value)?);
                    copied = reader.buffer_position() as usize;
                } else if matches!(event, Event::Start(_)) {
                    stack.push(name.as_ref().to_owned());
                }
            }
            Event::End(_) => {
                stack.pop();
            }
            Event::Eof => break,
            _ => {}
        }
    }
    ensure!(
        changes.is_empty(),
        "Only existing worksheet cells can be edited in place"
    );
    out.push_str(&xml[copied..]);
    Ok(out)
}

// Preserve the workbook's calculation mode. A manual workbook deliberately
// disables automatic calculation, so changing an input requires a separate export.
fn request_recalculation(xml: &str) -> Result<String> {
    const REQUEST: &str = "<calcPr fullCalcOnLoad=\"1\"/>";
    let mut reader = Reader::from_str(xml);
    let mut depth = 0usize;
    let mut insertion = None;
    loop {
        let begin = reader.buffer_position() as usize;
        let event = reader.read_event()?;
        match &event {
            Event::Start(tag) | Event::Empty(tag) => {
                let name = tag.name();
                if depth == 0 {
                    ensure!(
                        name.as_ref() == "workbook",
                        "Unsupported workbook namespace"
                    );
                } else if depth == 1 && name.as_ref() == "calcPr" {
                    ensure!(
                        attr(tag, "calcMode")?.as_deref() != Some("manual"),
                        "Workbook uses manual calculation. Use Export workbook to export the active table"
                    );
                    if matches!(attr(tag, "fullCalcOnLoad")?.as_deref(), Some("1" | "true")) {
                        return Ok(xml.to_owned());
                    }
                    let mut patched = BytesStart::new("calcPr");
                    for attribute in tag.attributes() {
                        let attribute = attribute?;
                        if attribute.key.as_ref() != "fullCalcOnLoad" {
                            patched.push_attribute(attribute);
                        }
                    }
                    patched.push_attribute(("fullCalcOnLoad", "1"));
                    let mut writer = quick_xml::Writer::new(Vec::new());
                    writer.write_event(if matches!(event, Event::Empty(_)) {
                        Event::Empty(patched)
                    } else {
                        Event::Start(patched)
                    })?;
                    let mut out = xml.to_owned();
                    out.replace_range(
                        begin..reader.buffer_position() as usize,
                        std::str::from_utf8(&writer.into_inner())?,
                    );
                    return Ok(out);
                } else if depth == 1
                    && matches!(
                        name.as_ref(),
                        "oleSize"
                            | "customWorkbookViews"
                            | "pivotCaches"
                            | "smartTagPr"
                            | "smartTagTypes"
                            | "webPublishing"
                            | "fileRecoveryPr"
                            | "webPublishObjects"
                            | "extLst"
                    )
                {
                    insertion.get_or_insert(begin);
                }
                if matches!(event, Event::Start(_)) {
                    depth += 1;
                }
            }
            Event::End(_) => {
                depth -= 1;
                if depth == 0 {
                    let mut out = xml.to_owned();
                    out.insert_str(insertion.unwrap_or(begin), REQUEST);
                    return Ok(out);
                }
            }
            Event::Eof => bail!("Incomplete workbook XML"),
            _ => {}
        }
    }
}

#[cfg(target_os = "macos")]
fn copy_file_metadata(source: &File, destination: &File) -> Result<()> {
    use std::os::fd::AsRawFd;
    // SAFETY: Both descriptors remain open and copyfile accepts a null state.
    let status = unsafe {
        libc::fcopyfile(
            source.as_raw_fd(),
            destination.as_raw_fd(),
            std::ptr::null_mut(),
            libc::COPYFILE_XATTR | libc::COPYFILE_ACL,
        )
    };
    if status != 0 {
        return Err(std::io::Error::last_os_error()).context("Preserving workbook metadata");
    }
    Ok(())
}

fn same_value(a: &CellValue, b: &CellValue) -> bool {
    match (a, b) {
        (CellValue::Int(i), CellValue::Float(f)) | (CellValue::Float(f), CellValue::Int(i)) => {
            i.unsigned_abs() <= 999_999_999_999_999 && *i as f64 == *f
        }
        _ => a == b,
    }
}

// Rename the workbook behind an in-place link, keeping the user's link in the
// directory. A Save As must never follow a link into a different workbook.
fn destination_for(source: &Path, path: &Path) -> Result<std::path::PathBuf> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) => {
            let target = path
                .canonicalize()
                .context("Resolving workbook destination")?;
            if meta.file_type().is_symlink() {
                ensure!(
                    source.canonicalize()? == target,
                    "Destination link points to a different file"
                );
            }
            Ok(target)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let parent = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            Ok(parent
                .canonicalize()?
                .join(path.file_name().context("Missing workbook name")?))
        }
        Err(e) => Err(e.into()),
    }
}

pub(super) fn save(path: &Path, sheet: Option<&str>, table: &DataTable) -> Result<()> {
    ensure!(
        path.extension()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.eq_ignore_ascii_case("xlsx")),
        "Use Save As to export this workbook to a new file"
    );
    ensure!(
        !table.structural_changes
            && table.row_offset == 0
            && table.total_rows.is_none_or(|n| n == table.row_count()),
        "Rows or columns changed. Use Export workbook to write the table as a sheet"
    );
    let source = table
        .source_path
        .as_deref()
        .map(Path::new)
        .context("No source workbook")?;
    let stamp = super::file_stamp(source).context("Missing source workbook")?;
    let destination_path = destination_for(source, path)?;
    let destination_stamp = super::file_stamp(&destination_path);
    let file = File::open(source)?;
    let mut archive = zip::ZipArchive::new(BufReader::new(&file))?;
    let mut expanded = 0_u64;
    for i in 0..archive.len() {
        let entry = archive.by_index_raw(i)?;
        ensure!(
            !entry
                .name()?
                .to_ascii_lowercase()
                .starts_with("_xmlsignatures/"),
            "Signed workbooks require a separate export"
        );
        expanded = expanded.saturating_add(entry.size());
    }
    ensure!(
        expanded <= octa::formats::compression::max_decompressed(),
        "Workbook exceeds the configured decompression limit"
    );
    let mut book = open_workbook_auto(source)?;
    let sheets = book.sheet_names();
    let name = match sheet {
        Some(s) => s,
        None if sheets.len() == 1 => &sheets[0],
        None => bail!("Select a named worksheet before saving"),
    };
    let origin = book.worksheet_range(name)?.start();
    drop(book);
    let base = ExcelReader.read_table(source, name)?;
    ensure!(
        base.columns
            .iter()
            .map(|c| &c.name)
            .eq(table.columns.iter().map(|c| &c.name))
            && base.rows.len() == table.rows.len()
            && base
                .rows
                .iter()
                .zip(&table.rows)
                .all(|(a, b)| a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_value(a, b))),
        "The worksheet or its row order changed. Reopen it before editing"
    );

    let mut changes = HashMap::new();
    for (&(r, c), value) in &table.edits {
        ensure!(
            r < table.row_count() && c < table.col_count(),
            "Edit outside the worksheet"
        );
        ensure!(
            matches!(
                base.rows[r][c],
                CellValue::Null
                    | CellValue::Bool(_)
                    | CellValue::Int(_)
                    | CellValue::Float(_)
                    | CellValue::String(_)
            ),
            "Editing a date or time in place requires an Excel editor"
        );
        ensure!(
            !base.formulas.contains_key(&(r, c)),
            "Formula cells require a separate export"
        );
        if same_value(value, &base.rows[r][c]) {
            continue;
        }
        let (row, col) = origin.context("Empty worksheet")?;
        let addr = rust_xlsxwriter::utility::row_col_to_cell(
            row + u32::try_from(r)? + 1,
            u16::try_from(col as usize + c)?,
        );
        changes.insert(addr, value);
    }
    let commit = |destination: File| -> Result<()> {
        #[cfg(target_os = "macos")]
        copy_file_metadata(&file, &destination)?;
        if destination_stamp.is_none() {
            destination.set_permissions(file.metadata()?.permissions())?;
        }
        destination.sync_all()?;
        ensure!(
            super::file_stamp(source) == Some(stamp),
            "Source workbook changed during save"
        );
        ensure!(
            destination_for(source, path)? == destination_path
                && super::file_stamp(&destination_path) == destination_stamp,
            "Destination workbook changed during save"
        );
        Ok(())
    };
    if changes.is_empty() {
        if original(table, path) {
            return Ok(());
        }
        return octa::formats::write_atomically(&destination_path, |tmp| {
            let mut copy = File::create_new(tmp)?;
            std::io::copy(&mut File::open(source)?, &mut copy)?;
            commit(copy)
        });
    }
    let workbook = part(&mut archive, "xl/workbook.xml")?;
    let recalc = request_recalculation(&workbook)?;
    let id = lookup(&workbook, "sheet", "name", name, "r:id")?;
    let target = lookup(
        &part(&mut archive, "xl/_rels/workbook.xml.rels")?,
        "Relationship",
        "Id",
        &id,
        "Target",
    )?;
    let member = target
        .strip_prefix('/')
        .map_or_else(|| format!("xl/{target}"), str::to_owned);
    ensure!(
        member.starts_with("xl/worksheets/") && !member.contains(".."),
        "Unsupported worksheet path"
    );
    let xml = patch(&part(&mut archive, &member)?, changes)?;
    octa::formats::write_atomically(&destination_path, |tmp| {
        let mut output = zip::ZipWriter::new(File::create_new(tmp)?);
        output.set_raw_comment(archive.comment().into())?;
        for i in 0..archive.len() {
            let entry = archive.by_index_raw(i)?;
            let name = entry.name()?;
            let replacement = if name == member {
                Some(&xml)
            } else if name == "xl/workbook.xml" && recalc != workbook {
                Some(&recalc)
            } else {
                None
            };
            if let Some(replacement) = replacement {
                output.start_file(name.as_ref(), entry.options())?;
                output.write_all(replacement.as_bytes())?;
            } else {
                output.raw_copy_file(entry)?;
            }
        }
        commit(output.finish()?)
    })
}

#[cfg(test)]
#[path = "workbook_tests.rs"]
mod tests;
