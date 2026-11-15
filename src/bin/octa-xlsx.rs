//! Bounded CSV preview of a modern Excel worksheet, in its original cell coordinates.
use calamine::{DataRef, Reader, Xlsx, open_workbook};
use std::{error::Error, ffi::OsString, io, path::Path, process::ExitCode};

const HELP: &str = "octa-xlsx FILE ROWS [SHEET]\nPrint up to ROWS data rows plus their header as CSV. Supports XLSX and XLSM.";

fn preview(path: &Path, limit: u32, sheet: Option<&str>) -> Result<(), Box<dyn Error>> {
    let mut book: Xlsx<_> = open_workbook(path)?;
    let name = match sheet {
        Some(name) => name.to_owned(),
        None => book
            .sheet_names()
            .into_iter()
            .next()
            .ok_or("empty workbook")?,
    };
    let mut cells = book.worksheet_cells_reader(&name)?;
    let mut first = None;
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut width = 0;
    while let Some(cell) = cells.next_cell()? {
        let (r, c) = cell.get_position();
        if r >= 1_048_576 || c >= 16_384 {
            return Err("cell outside Excel worksheet bounds".into());
        }
        if first.is_some_and(|start: u32| r.saturating_sub(start) > limit) {
            rows.resize_with(limit as usize + 1, Vec::new);
            break;
        }
        if matches!(cell.get_value(), DataRef::Empty) {
            continue;
        }
        let start = *first.get_or_insert(r);
        let ri = r
            .checked_sub(start)
            .ok_or("worksheet rows are out of order")? as usize;
        if rows.len() <= ri {
            rows.resize_with(ri + 1, Vec::new);
        }
        let row = &mut rows[ri];
        let column = c as usize;
        if row.len() <= column {
            row.resize_with(column + 1, String::new);
        }
        row[column] = match cell.get_value() {
            DataRef::String(s) | DataRef::DurationIso(s) | DataRef::DateTimeIso(s) => s.clone(),
            DataRef::SharedString(s) => (*s).to_owned(),
            value => calamine::Data::from(value.clone()).to_string(),
        };
        width = width.max(row.len());
    }
    let mut out = csv::Writer::from_writer(io::stdout().lock());
    for row in rows {
        out.write_record(
            row.iter()
                .map(String::as_str)
                .chain(std::iter::repeat_n("", width - row.len())),
        )?;
    }
    out.flush()?;
    Ok(())
}

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if matches!(args.as_slice(), [arg] if arg == "--help" || arg == "-h") {
        println!("{HELP}");
        return Ok(());
    }
    let [file, limit, rest @ ..] = args.as_slice() else {
        return Err(HELP.into());
    };
    if rest.len() > 1 {
        return Err(HELP.into());
    }
    let n = limit
        .to_str()
        .and_then(|s| s.parse::<u32>().ok())
        .ok_or("expected a non-negative row count")?;
    let sheet = rest
        .first()
        .map(|s| s.to_str().ok_or("invalid sheet name"))
        .transpose()?;
    preview(Path::new(file), n, sheet)
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}
