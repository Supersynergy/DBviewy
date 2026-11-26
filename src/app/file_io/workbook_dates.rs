//! Calendar values for the cell-only workbook patch. Never change its styles.
use anyhow::{Context, Result, bail, ensure};
use chrono::{Datelike, NaiveDate, NaiveDateTime, Timelike};
use octa::data::CellValue;
use quick_xml::{Reader, events::Event};
use rust_xlsxwriter::ExcelDateTime;

pub(super) fn calendar_value(value: &CellValue) -> bool {
    matches!(value, CellValue::Date(_) | CellValue::DateTime(_))
}

pub(super) fn parse(value: &CellValue) -> Option<NaiveDateTime> {
    let text = match value {
        CellValue::Date(text) | CellValue::DateTime(text) => text.trim(),
        _ => return None,
    };
    if text.len() == 10 {
        return NaiveDate::parse_from_str(text, "%Y-%m-%d")
            .ok()?
            .and_hms_opt(0, 0, 0);
    }
    if !matches!(value, CellValue::DateTime(_)) {
        return None;
    }
    [
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%dT%H:%M",
    ]
    .iter()
    .find_map(|format| NaiveDateTime::parse_from_str(text, format).ok())
}

pub(super) fn uses_1904_epoch(xml: &str) -> Result<bool> {
    let mut reader = Reader::from_str(xml);
    let mut depth = 0;
    let mut epoch = None;
    loop {
        let event = reader.read_event()?;
        match &event {
            Event::Start(tag) | Event::Empty(tag) => {
                if tag.local_name().as_ref() == "workbookPr" {
                    // Like the XLSX reader, accept a namespace prefix and
                    // use the literal attribute rather than unescaping it.
                    let date_1904 = tag.try_get_attribute("date1904")?;
                    if depth == 1 {
                        ensure!(epoch.is_none(), "Ambiguous workbook date system");
                        epoch = Some(match date_1904.as_ref().map(|attr| attr.value.as_ref()) {
                            None | Some("0" | "false") => false,
                            Some("1" | "true") => true,
                            _ => bail!("Unrecognised workbook date system"),
                        });
                    } else {
                        ensure!(date_1904.is_none(), "Nested workbook date system");
                    }
                }
                depth += usize::from(matches!(event, Event::Start(_)));
            }
            Event::End(_) => depth -= 1,
            Event::Eof => return Ok(epoch.unwrap_or(false)),
            _ => {}
        }
    }
}

pub(super) fn encode(
    value: &CellValue,
    address: &str,
    iso: bool,
    date_1904: bool,
) -> Result<(&'static str, String)> {
    let time = parse(value).with_context(|| {
        format!("Use YYYY-MM-DD or a date and local time without an offset in {address}")
    })?;
    ensure!(
        (1900..=9999).contains(&time.year()),
        "Date outside Excel's calendar in {address}"
    );
    let ns = time.nanosecond();
    ensure!(
        ns < 1_000_000_000 && ns.is_multiple_of(1_000_000),
        "Use at most millisecond precision in {address}"
    );
    if iso {
        // The worksheet already used the ISO date type. Keep its encoding
        // even when it has no numerical date style (or uses the 1904 epoch).
        return Ok((
            "d",
            format!("<v>{}</v>", time.format("%Y-%m-%dT%H:%M:%S%.f")),
        ));
    }
    let date = ExcelDateTime::from_ymd(time.year() as u16, time.month() as u8, time.day() as u8)?;
    let serial = date
        .and_hms_milli(
            time.hour() as u16,
            time.minute() as u8,
            time.second() as u8,
            (ns / 1_000_000) as u16,
        )?
        .to_excel()
        - if date_1904 { 1462.0 } else { 0.0 };
    // The numeric reader treats fractional values below 1 as clock times.
    ensure!(
        serial >= 1.0,
        "Date outside the workbook's calendar in {address}"
    );
    Ok(("n", format!("<v>{serial}</v>")))
}
