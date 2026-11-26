//! Workday counting and offsets for spreadsheet formulas.
use chrono::{Days, NaiveDate};

use crate::data::time_calc::WorkCalendar;

use super::control::{arguments, unparen};
use super::criteria::rectangle;
use super::date::{FIRST, LAST, date_argument, date_cell, serial, whole};
use super::{CellValue, Evaluation, FormulaToken as Token, eval_complete};

const WEEKEND: [bool; 7] = [false, false, false, false, false, true, true];

#[derive(Clone, Copy, Debug)]
pub(super) enum Kind {
    Count,
    Offset,
}

pub(super) fn evaluate(
    kind: Kind,
    intl: bool,
    tokens: &[Token],
    pos: usize,
    context: &mut Evaluation<'_>,
) -> Option<(f64, usize)> {
    let (start, second, weekend, holidays, end) = if intl {
        let ([start, second, weekend, holidays], end) = arguments(tokens, pos)?;
        (start?, second?, weekend, holidays, end)
    } else {
        let ([start, second, holidays], end) = arguments(tokens, pos)?;
        (start?, second?, None, holidays, end)
    };
    let start = date_argument(start, context)?;
    let off = weekend_mask(weekend, context)?;
    let holidays = holiday_dates(holidays, context)?;
    let date_or_days = match kind {
        Kind::Count => {
            let finish = date_argument(second, context)?;
            if context.table.is_none() {
                return Some((0.0, end));
            }
            WorkCalendar::new(off, &holidays).days_between(start, finish, true) as f64
        }
        Kind::Offset => {
            let offset = eval_complete(second, context)?;
            if context.table.is_none() {
                return Some((0.0, end));
            }
            if off.iter().all(|day_off| *day_off) {
                return None;
            }
            serial(add_workdays(
                &WorkCalendar::new(off, &holidays),
                start,
                whole(offset)?,
            )?)
        }
    };
    Some((date_or_days, end))
}

fn weekend_mask(arg: Option<&[Token]>, context: &mut Evaluation<'_>) -> Option<[bool; 7]> {
    let Some(tokens) = arg.filter(|part| !part.is_empty()).map(unparen) else {
        return Some(WEEKEND);
    };
    let text = match tokens {
        [Token::Text(text)] => Some(text),
        [Token::CellRef(row, col)] => context.table.and_then(|table| match table.get(*row, *col) {
            Some(CellValue::String(text)) => Some(text),
            _ => None,
        }),
        _ => None,
    };
    if let Some(text) = text {
        if context.table.is_none() {
            return Some(WEEKEND);
        }
        let bytes: [u8; 7] = text.as_bytes().try_into().ok()?;
        return bytes
            .iter()
            .all(|b| matches!(b, b'0' | b'1'))
            .then(|| bytes.map(|b| b == b'1'));
    }
    let code = eval_complete(tokens, context)?;
    if context.table.is_none() {
        return Some(WEEKEND);
    }
    let mut off = [false; 7];
    match code {
        1.0..=7.0 if code.fract() == 0.0 => {
            let start = (code as usize + 4) % 7;
            off[start] = true;
            off[(start + 1) % 7] = true;
        }
        11.0..=17.0 if code.fract() == 0.0 => off[(code as usize - 5) % 7] = true,
        _ => return None,
    }
    Some(off)
}

fn holiday_dates(arg: Option<&[Token]>, context: &mut Evaluation<'_>) -> Option<Vec<NaiveDate>> {
    let Some(tokens) = arg.filter(|part| !part.is_empty()).map(unparen) else {
        return Some(Vec::new());
    };
    if let Some((range, end)) = rectangle(tokens, 0)
        && end == tokens.len()
    {
        let mut dates = Vec::new();
        if let Some(table) = context.table {
            let (rows, cols) = range.loaded_shape(table);
            for row in 0..rows {
                for col in 0..cols {
                    let (r, c) = range.at(row, col);
                    match table.get(r, c) {
                        None | Some(CellValue::Null) => {}
                        Some(CellValue::String(s)) if s.is_empty() => {}
                        _ => dates.push(date_cell(context, r, c)?),
                    }
                }
            }
        }
        Some(dates)
    } else {
        Some(vec![date_argument(tokens, context)?])
    }
}

fn add_workdays(calendar: &WorkCalendar, start: NaiveDate, offset: i64) -> Option<NaiveDate> {
    if offset == 0 {
        return Some(start);
    }
    let forward = offset > 0;
    let wanted = offset.unsigned_abs();
    // Distance in calendar days, measured from the starting date. Searching
    // for the first sufficient count lands on a working day and skips over
    // arbitrarily long ranges without walking one date at a time.
    let bound = if forward { LAST - start } else { start - FIRST }.num_days() as u64;
    if wanted > bound {
        return None;
    }
    let date_at = |n| {
        if forward {
            start.checked_add_days(Days::new(n))
        } else {
            start.checked_sub_days(Days::new(n))
        }
    };
    let first = date_at(1)?;
    let reaches = |n| {
        let date = date_at(n)?;
        let count = if forward {
            calendar.days_between(first, date, true)
        } else {
            calendar.days_between(date, first, true)
        };
        Some(count as u64 >= wanted)
    };
    if !reaches(bound)? {
        return None;
    }
    let (mut lo, mut hi) = (1, bound);
    while lo < hi {
        let middle = lo + (hi - lo) / 2;
        if reaches(middle)? {
            hi = middle;
        } else {
            lo = middle + 1;
        }
    }
    date_at(lo)
}
