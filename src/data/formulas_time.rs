//! Clock functions and the scalar remainder used for overnight shifts.

use chrono::Timelike;

use super::control::{arguments, unparen};
use super::{CellValue, Evaluation, FormulaBadCell, FormulaToken as Token, eval_complete, snippet};
use crate::data::times::{NANOS_PER_DAY, NANOS_PER_SEC, parse_time};

const SECONDS_PER_DAY: i64 = NANOS_PER_DAY / NANOS_PER_SEC;

#[derive(Clone, Copy, Debug)]
pub(super) enum Clock {
    Time,
    TimeValue,
    Hour,
    Minute,
    Second,
    Modulo,
}

pub(super) fn evaluate(
    kind: Clock,
    tokens: &[Token],
    pos: usize,
    context: &mut Evaluation<'_>,
) -> Option<(f64, usize)> {
    match kind {
        Clock::Time => {
            let ([h, m, s], end) = arguments(tokens, pos)?;
            let h = eval_complete(h?, context)?;
            let m = eval_complete(m?, context)?;
            let s = eval_complete(s?, context)?;
            if context.table.is_none() {
                return Some((0.0, end));
            }
            let part = |v: f64| (v >= 0.0 && v.trunc() <= 32_767.0).then_some(v as i64);
            let seconds = part(h)? * 3600 + part(m)? * 60 + part(s)?;
            Some((
                (seconds % SECONDS_PER_DAY) as f64 / SECONDS_PER_DAY as f64,
                end,
            ))
        }
        Clock::Modulo => {
            let ([number, divisor], end) = arguments(tokens, pos)?;
            let number = eval_complete(number?, context)?;
            let divisor = eval_complete(divisor?, context)?;
            if context.table.is_none() {
                return Some((0.0, end));
            }
            if divisor == 0.0 || !divisor.is_finite() || !number.is_finite() {
                return None;
            }
            let mut rem = number % divisor;
            if rem != 0.0 && rem.is_sign_negative() != divisor.is_sign_negative() {
                rem += divisor;
            }
            // Subtracting a tiny remainder may round to the divisor. The
            // mathematical result is still strictly inside its magnitude.
            if rem == divisor {
                rem = if divisor > 0.0 {
                    divisor.next_down()
                } else {
                    divisor.next_up()
                };
            }
            Some((if rem == 0.0 { 0.0 } else { rem }, end))
        }
        _ => {
            let ([arg], end) = arguments(tokens, pos)?;
            let value_only = matches!(kind, Clock::TimeValue);
            let nanos = clock_argument(arg?, value_only, context)?;
            let seconds = nanos / NANOS_PER_SEC;
            let value = match kind {
                Clock::TimeValue => nanos as f64 / NANOS_PER_DAY as f64,
                Clock::Hour => (seconds / 3600) as f64,
                Clock::Minute => ((seconds / 60) % 60) as f64,
                Clock::Second => (seconds % 60) as f64,
                _ => unreachable!(),
            };
            Some((value, end))
        }
    }
}

fn clock_argument(tokens: &[Token], value_only: bool, context: &mut Evaluation<'_>) -> Option<i64> {
    let tokens = unparen(tokens);
    if context.table.is_none() {
        if !matches!(tokens, [Token::Text(_)]) {
            eval_complete(tokens, context)?;
        }
        return Some(0);
    }
    match tokens {
        [Token::Text(text)] => text_time(text),
        [Token::Boolean(_)] => None,
        [Token::CellRef(row, col)] => {
            let cell = context.table?.get(*row, *col)?;
            let value = match cell {
                CellValue::Time(ns) if (0..NANOS_PER_DAY).contains(ns) => Some(*ns),
                CellValue::DateTime(text) | CellValue::String(text) => text_time(text),
                CellValue::Duration(ns) if !value_only && *ns >= 0 => Some(ns % NANOS_PER_DAY),
                CellValue::Date(_) if !value_only => super::date::criterion_serial(cell).map(|_| 0),
                CellValue::Int(n) if !value_only => numeric_time(*n as f64),
                CellValue::Float(n) if !value_only => numeric_time(*n),
                _ => None,
            };
            if value.is_none() && context.bad.is_none() {
                context.bad = Some(FormulaBadCell {
                    row: *row,
                    col: *col,
                    content: snippet(&cell.to_string()),
                });
            }
            value
        }
        _ => {
            // Validate the expression even when its numeric result will not
            // be accepted by TIMEVALUE, just as in the inactive branch.
            let value = eval_complete(tokens, context)?;
            if value_only {
                None
            } else {
                numeric_time(value)
            }
        }
    }
}

fn text_time(text: &str) -> Option<i64> {
    if let Some(ns) = parse_time(text) {
        return Some(ns);
    }
    let (time, has_clock) = super::date::local_datetime(text)?;
    // The timestamp parser also accepts dates and leap seconds. Neither is
    // a clock reading that Excel's fractional-day format can represent.
    let ns = time.nanosecond();
    (has_clock && ns < NANOS_PER_SEC as u32)
        .then(|| i64::from(time.num_seconds_from_midnight()) * NANOS_PER_SEC + i64::from(ns))
}

fn numeric_time(serial: f64) -> Option<i64> {
    if !serial.is_finite() || serial < 0.0 {
        return None;
    }
    let day = serial.trunc();
    let seconds = serial.fract() * SECONDS_PER_DAY as f64;
    // Undo rounding only when the nearest whole second serialises to this
    // exact float. Genuinely fractional seconds keep their lower second,
    // and the last fraction of a day never rolls into the next midnight.
    let nearest = seconds.round();
    let whole =
        if nearest < SECONDS_PER_DAY as f64 && day + nearest / SECONDS_PER_DAY as f64 == serial {
            nearest as i64
        } else {
            (seconds as i64).min(SECONDS_PER_DAY - 1)
        };
    Some(whole * NANOS_PER_SEC)
}
