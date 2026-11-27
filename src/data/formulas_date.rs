//! Gregorian calendar functions for the numeric formula evaluator.
use chrono::{DateTime, Datelike, Days, Months, NaiveDate, NaiveDateTime, TimeDelta, Timelike};

use super::control::{arguments, unparen};
use super::{CellValue, Evaluation, FormulaBadCell, FormulaToken as Token, eval_complete, snippet};

pub(super) const FIRST: NaiveDate = NaiveDate::from_ymd_opt(1900, 1, 1).unwrap();
pub(super) const LAST: NaiveDate = NaiveDate::from_ymd_opt(9999, 12, 31).unwrap();
const MARCH_1900: NaiveDate = NaiveDate::from_ymd_opt(1900, 3, 1).unwrap();
const MAX_SERIAL: f64 = 2_958_465.0;

#[derive(Clone, Copy, Debug)]
pub(super) enum Calendar {
    Date,
    Year,
    Month,
    Day,
    Days,
    Weekday,
    MonthShift,
    MonthEnd,
    IsoWeek,
}

pub(super) fn evaluate(
    kind: Calendar,
    tokens: &[Token],
    pos: usize,
    context: &mut Evaluation<'_>,
) -> Option<(f64, usize)> {
    let (value, end) = match kind {
        Calendar::Date => {
            let ([year, month, day], end) = arguments(tokens, pos)?;
            let year = eval_complete(year?, context)?;
            let month = eval_complete(month?, context)?;
            let day = eval_complete(day?, context)?;
            if context.table.is_none() {
                return Some((0.0, end));
            }
            let date = normalise_date(whole(year)?, whole(month)?, whole(day)?)?;
            (serial(date), end)
        }
        Calendar::MonthShift | Calendar::MonthEnd => {
            let ([date, months], end) = arguments(tokens, pos)?;
            let date = date_argument(date?, context)?;
            let months = eval_complete(months?, context)?;
            if context.table.is_none() {
                return Some((0.0, end));
            }
            let date = move_months(date, whole(months)?)?;
            let result = if matches!(kind, Calendar::MonthEnd) {
                date.with_day(1)?
                    .checked_add_months(Months::new(1))?
                    .pred_opt()?
            } else {
                date
            };
            return (FIRST..=LAST)
                .contains(&result)
                .then(|| (serial(result), end));
        }
        Calendar::Days => {
            let ([end_date, start_date], end) = arguments(tokens, pos)?;
            let end_date = date_argument(end_date?, context)?;
            let start_date = date_argument(start_date?, context)?;
            ((end_date - start_date).num_days() as f64, end)
        }
        _ => {
            let ([date, numbering], end) = arguments(tokens, pos)?;
            let date = date_argument(date?, context)?;
            let value = match kind {
                Calendar::Year if numbering.is_none() => f64::from(date.year()),
                Calendar::Month if numbering.is_none() => f64::from(date.month()),
                Calendar::Day if numbering.is_none() => f64::from(date.day()),
                Calendar::IsoWeek if numbering.is_none() => f64::from(date.iso_week().week()),
                Calendar::Weekday => {
                    let numbering = match numbering.filter(|arg| !arg.is_empty()) {
                        Some(arg) => eval_complete(arg, context)?,
                        None => 1.0,
                    };
                    if context.table.is_none() {
                        return Some((0.0, end));
                    }
                    let monday = date.weekday().num_days_from_monday();
                    f64::from(match numbering {
                        1.0 | 17.0 => (monday + 1) % 7 + 1,
                        2.0 | 11.0 => monday + 1,
                        3.0 => monday,
                        12.0..=16.0 if numbering.fract() == 0.0 => {
                            (monday + 18 - numbering as u32) % 7 + 1
                        }
                        _ => return None,
                    })
                }
                _ => return None,
            };
            (value, end)
        }
    };
    Some((value, end))
}

fn move_months(date: NaiveDate, months: i64) -> Option<NaiveDate> {
    let steps = Months::new(u32::try_from(months.unsigned_abs()).ok()?);
    // Chrono keeps the original day, clamping it only if the target
    // month is too short. This is a calendar offset, not a day count.
    if months < 0 {
        date.checked_sub_months(steps)
    } else {
        date.checked_add_months(steps)
    }
}

pub(super) fn whole(value: f64) -> Option<i64> {
    (i64::MIN as f64..i64::MAX as f64)
        .contains(&value)
        .then_some(value as i64)
}

pub(super) fn serial(date: NaiveDate) -> f64 {
    ((date - FIRST).num_days() + 1 + i64::from(date >= MARCH_1900)) as f64
}

/// Parse a date or timestamp without shifting an explicit UTC offset.
pub(super) fn local_datetime(text: &str) -> Option<(NaiveDateTime, bool)> {
    let text = text.trim();
    let offset = text
        .as_bytes()
        .get(11..)
        .is_some_and(|clock| clock.iter().any(|b| matches!(b, b'+' | b'-' | b'Z' | b'z')));
    if !offset {
        return crate::data::time_calc::parse_any(text);
    }
    let time = DateTime::parse_from_rfc3339(text).ok().or_else(|| {
        [
            "%Y-%m-%d %H:%M:%S%.f%z",
            "%Y-%m-%dT%H:%M:%S%.f%z",
            "%Y-%m-%d %H:%M%z",
            "%Y-%m-%dT%H:%M%z",
        ]
        .iter()
        .find_map(|format| DateTime::parse_from_str(text, format).ok())
    })?;
    Some((time.naive_local(), true))
}

/// The serial used by numeric criteria, including a timestamp's local
/// clock time. Plain strings do not participate in this coercion.
pub(super) fn criterion_serial(cell: &CellValue) -> Option<f64> {
    let (day, fraction) = match cell {
        CellValue::Date(text) => (iso_date(text)?, 0.0),
        CellValue::DateTime(text) => {
            let (time, _) = local_datetime(text)?;
            let ns = time.nanosecond();
            // Excel serials do not represent leap seconds.
            if ns >= 1_000_000_000 {
                return None;
            }
            let seconds = f64::from(time.num_seconds_from_midnight()) + f64::from(ns) / 1e9;
            (time.date(), seconds / 86_400.0)
        }
        _ => return None,
    };
    if !(FIRST..=LAST).contains(&day) {
        return None;
    }
    let start = serial(day);
    if fraction == 0.0 {
        Some(start)
    } else {
        // A sub-microsecond time can round to midnight at serial-number
        // precision. Keep it on the correct side of both day boundaries.
        Some((start + fraction).clamp(start.next_up(), (start + 1.0).next_down()))
    }
}

fn normalise_date(year: i64, month: i64, day: i64) -> Option<NaiveDate> {
    let year = match year {
        0..=1899 => year + 1900,
        1900..=9999 => year,
        _ => return None,
    };
    let month = (year * 12).checked_add(month.checked_sub(1)?)?;
    let first = NaiveDate::from_ymd_opt(
        i32::try_from(month.div_euclid(12)).ok()?,
        u32::try_from(month.rem_euclid(12) + 1).ok()?,
        1,
    )?;
    let date = first.checked_add_signed(TimeDelta::try_days(day.checked_sub(1)?)?)?;
    (FIRST..=LAST).contains(&date).then_some(date)
}

fn from_serial(value: f64) -> Option<NaiveDate> {
    let serial = value.trunc();
    if !(1.0..=MAX_SERIAL).contains(&serial) || serial == 60.0 {
        return None;
    }
    // Leave a gap for Excel's deliberately fictitious 1900-02-29.
    FIRST.checked_add_days(Days::new(serial as u64 - 1 - u64::from(serial > 60.0)))
}

fn iso_date(text: &str) -> Option<NaiveDate> {
    let text = text.trim();
    if text.len() != 10
        || text.as_bytes()[4] != b'-'
        || text.as_bytes()[7] != b'-'
        || text
            .bytes()
            .enumerate()
            .any(|(i, b)| i != 4 && i != 7 && !b.is_ascii_digit())
    {
        return None;
    }
    NaiveDate::parse_from_str(text, "%Y-%m-%d").ok()
}

pub(super) fn date_argument(tokens: &[Token], context: &mut Evaluation<'_>) -> Option<NaiveDate> {
    let tokens = unparen(tokens);
    if context.table.is_none() {
        // Syntax-only validation must not reject an inactive argument for
        // its value (invalid ISO text, missing address or an invalid serial).
        if !matches!(
            tokens,
            [Token::Text(_) | Token::CellRef(..) | Token::Boolean(_)]
        ) {
            eval_complete(tokens, context)?;
        }
        return Some(FIRST);
    }
    match tokens {
        [Token::Text(text)] => iso_date(text).filter(|d| (FIRST..=LAST).contains(d)),
        [Token::Boolean(_)] => None,
        [Token::CellRef(row, col)] => date_cell(context, *row, *col),
        _ => from_serial(eval_complete(tokens, context)?),
    }
}

pub(super) fn date_cell(context: &mut Evaluation<'_>, row: usize, col: usize) -> Option<NaiveDate> {
    let cell = context.table?.get(row, col)?;
    let date = match cell {
        CellValue::Date(text) | CellValue::String(text) => iso_date(text),
        CellValue::DateTime(text) => DateTime::parse_from_rfc3339(text)
            .ok()
            .map(|time| time.date_naive())
            .or_else(|| crate::data::time_calc::parse_date(text)),
        CellValue::Int(value) => from_serial(*value as f64),
        CellValue::Float(value) => from_serial(*value),
        _ => None,
    }
    .filter(|d| (FIRST..=LAST).contains(d));
    if date.is_none() && context.bad.is_none() {
        context.bad = Some(FormulaBadCell {
            row,
            col,
            content: snippet(&cell.to_string()),
        });
    }
    date
}
