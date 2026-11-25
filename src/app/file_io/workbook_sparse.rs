//! Insert missing scalar cells only in unambiguous existing worksheet rows.
use super::{XmlChange, attr, scalar_cell};
use anyhow::{Context, Result, ensure};
use octa::data::CellValue;
use quick_xml::{Reader, events::Event};
use std::collections::{BTreeMap, HashMap};

const MAX_ROW: u32 = 1_048_576;
const MAX_COLUMN: u16 = 16_384;
const ROW: [&str; 3] = ["worksheet", "sheetData", "row"];
type Coordinate = (u32, u16);
type Pending<'a> = BTreeMap<Coordinate, (String, &'a CellValue)>;

fn row_number(raw: &str) -> Option<u32> {
    if raw.starts_with('0') || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    raw.parse().ok().filter(|r| (1..=MAX_ROW).contains(r))
}

fn coordinate(address: &str) -> Option<Coordinate> {
    let split = address.find(|c: char| !c.is_ascii_uppercase())?;
    let (letters, digits) = address.split_at(split);
    if !(1..=3).contains(&letters.len()) {
        return None;
    }
    let column = letters
        .bytes()
        .fold(0u32, |n, b| n * 26 + u32::from(b - b'A' + 1));
    (column <= u32::from(MAX_COLUMN)).then_some((row_number(digits)?, column as u16))
}

fn check_merge(reference: &str, pending: &Pending<'_>) -> Result<()> {
    let invalid = "Unrecognised merged range in the worksheet";
    let (first, last) = reference.split_once(':').unwrap_or((reference, reference));
    let (top, left) = coordinate(first).context(invalid)?;
    let (bottom, right) = coordinate(last).context(invalid)?;
    ensure!(top <= bottom && left <= right, "{invalid}");
    ensure!(
        !pending
            .range((top, 1)..=(bottom, MAX_COLUMN))
            .any(|(&(_, column), _)| (left..=right).contains(&column)),
        "Cannot fill a blank cell inside merged range {reference}"
    );
    Ok(())
}

#[derive(Default)]
struct Row<'a> {
    number: u32,
    span: Option<String>,
    cells: Vec<(u16, usize)>,
    edits: Vec<(u16, &'a str, &'a CellValue)>,
}

impl Row<'_> {
    fn finish(&self, end: usize, insertions: &mut Vec<XmlChange>) -> Result<()> {
        for &(column, address, value) in &self.edits {
            // `spans` is an optional column hint. Do not leave it inconsistent
            // or change row metadata to support this cell-only write.
            ensure!(
                self.span.as_ref().is_none_or(|span| {
                    span.split_ascii_whitespace().any(|item| {
                        item.split_once(':').is_some_and(|(first, last)| {
                            match (first.parse::<u16>(), last.parse::<u16>()) {
                                (Ok(first), Ok(last)) => {
                                    first > 0
                                        && last <= MAX_COLUMN
                                        && (first..=last).contains(&column)
                                }
                                _ => false,
                            }
                        })
                    })
                }),
                "Blank cell {address} falls outside the worksheet's row span"
            );
            let next = self.cells.partition_point(|&(col, _)| col < column);
            let offset = if let Some(&(existing, offset)) = self.cells.get(next) {
                ensure!(existing != column, "Ambiguous worksheet cell {address}");
                offset
            } else {
                end
            };
            ensure!(
                !matches!(value, CellValue::Null),
                "Cannot clear unrepresented cell {address}"
            );
            insertions.push(XmlChange {
                start: offset,
                end: offset,
                cell: scalar_cell(address, None, value)?,
            });
        }
        Ok(())
    }
}

pub(super) fn plan(xml: &str, changes: HashMap<String, &CellValue>) -> Result<Vec<XmlChange>> {
    // Edits must already be inside the loaded data rectangle. Do not infer
    // row/cell coordinates or create rows, dimensions or formatting here.
    let mut pending = Pending::new();
    for (address, value) in changes {
        let coord = coordinate(&address).context("Invalid worksheet cell address")?;
        pending.insert(coord, (address, value));
    }
    let mut reader = Reader::from_str(xml);
    let mut stack = Vec::new();
    let mut previous_row = 0;
    let mut row = Row::default();
    let mut insertions = Vec::with_capacity(pending.len());
    loop {
        let offset = reader.buffer_position() as usize;
        let event = reader.read_event()?;
        match event {
            Event::Start(ref tag) | Event::Empty(ref tag) => {
                let name = tag.name();
                if stack == ["worksheet", "sheetData"] && name.as_ref() == "row" {
                    let number = attr(tag, "r")?
                        .as_deref()
                        .and_then(row_number)
                        .context("Cannot fill blanks in a worksheet with unnumbered rows")?;
                    ensure!(number > previous_row, "Ambiguous worksheet row {number}");
                    previous_row = number;
                    row.number = number;
                    row.edits.clear();
                    row.cells.clear();
                    if matches!(event, Event::Start(_)) {
                        row.edits.extend(
                            pending
                                .range((number, 1)..=(number, MAX_COLUMN))
                                .map(|(&(_, col), (r, v))| (col, r.as_str(), *v)),
                        );
                        if !row.edits.is_empty() {
                            row.span = attr(tag, "spans")?;
                        }
                    }
                } else if stack == ROW && !row.edits.is_empty() {
                    ensure!(
                        name.as_ref() == "c",
                        "Cannot fill blanks in a row with unsupported metadata"
                    );
                    let (r, col) = attr(tag, "r")?
                        .as_deref()
                        .and_then(coordinate)
                        .context("Cannot fill blanks in a row with unnumbered cells")?;
                    ensure!(
                        r == row.number && row.cells.last().is_none_or(|&(prev, _)| prev < col),
                        "Ambiguous worksheet cells in row {}",
                        row.number
                    );
                    row.cells.push((col, offset));
                } else if name.local_name().as_ref() == "mergeCell" {
                    check_merge(
                        &attr(tag, "ref")?.context("Missing merged range reference")?,
                        &pending,
                    )?;
                }
                if matches!(event, Event::Start(_)) {
                    stack.push(name.as_ref().to_owned());
                }
            }
            Event::End(_) => {
                if stack == ROW && !row.edits.is_empty() {
                    row.finish(offset, &mut insertions)?;
                }
                stack.pop();
            }
            Event::Eof => break,
            _ => {}
        }
    }
    ensure!(
        insertions.len() == pending.len(),
        "Blank cells can only be filled in existing worksheet rows"
    );
    Ok(insertions)
}
