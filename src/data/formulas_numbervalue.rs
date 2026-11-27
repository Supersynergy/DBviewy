//! Convert imported numbers without depending on process or display locale.

use std::fmt::Write;

use super::control::{arguments, unparen};
use super::{CellValue, Evaluation, FormulaBadCell, FormulaToken as Token, eval_complete, snippet};

enum Separator {
    Default,
    Empty,
    Character(char),
}

fn separator(input: Option<&[Token]>, context: &Evaluation<'_>) -> Option<Separator> {
    let value = match input.map(unparen).unwrap_or_default() {
        [] => return Some(Separator::Default),
        [Token::Text(_)] | [Token::CellRef(..)] if context.table.is_none() => {
            return Some(Separator::Default);
        }
        [Token::Text(s)] => s,
        [Token::CellRef(row, col)] => match context.table?.get(*row, *col)? {
            CellValue::String(s) => s,
            CellValue::Null => return Some(Separator::Empty),
            _ => return None,
        },
        _ => return None,
    };
    match value.chars().next() {
        Some(ch) if !ch.is_ascii_digit() && !matches!(ch, '+' | '-' | '%' | 'e' | 'E') => {
            Some(Separator::Character(ch))
        }
        Some(_) => None,
        None => Some(Separator::Empty),
    }
}

pub(super) fn evaluate(
    tokens: &[Token],
    pos: usize,
    context: &mut Evaluation<'_>,
) -> Option<(f64, usize)> {
    let ([value, decimal, group], end) = arguments(tokens, pos)?;
    let value = unparen(value?);
    let decimal = match separator(decimal, context)? {
        Separator::Default => '.',
        Separator::Character(ch) if !ch.is_whitespace() => ch,
        _ => return None,
    };
    let group = match separator(group, context)? {
        Separator::Default => Some(if decimal == ',' { '.' } else { ',' }),
        Separator::Empty => None,
        Separator::Character(ch) => Some(ch),
    };
    if group == Some(decimal) {
        return None;
    }
    let Some(table) = context.table else {
        if !matches!(value, [Token::Text(_) | Token::CellRef(..)]) {
            eval_complete(value, context)?;
        }
        return Some((0.0, end));
    };
    let parsed = match value {
        [Token::Text(text)] => parse(text, decimal, group),
        [Token::CellRef(row, col)] => {
            let cell = table.get(*row, *col)?;
            let number = match cell {
                CellValue::Int(n) => Some(*n as f64),
                CellValue::Float(n) => n.is_finite().then_some(*n),
                CellValue::Null => Some(0.0),
                CellValue::String(s) => parse(s, decimal, group),
                _ => None,
            };
            if number.is_none() && context.bad.is_none() {
                let content = match cell {
                    CellValue::String(s) => snippet(s),
                    other => snippet(&other.to_string()),
                };
                context.bad = Some(FormulaBadCell {
                    row: *row,
                    col: *col,
                    content,
                });
            }
            number
        }
        _ => eval_complete(value, context).filter(|n| n.is_finite()),
    };
    Some((parsed?, end))
}

fn parse(input: &str, decimal: char, group: Option<char>) -> Option<f64> {
    let input = input.trim();
    if input.is_empty() {
        return Some(0.0);
    }
    // Ordinary ungrouped decimals need no buffer.
    if decimal == '.'
        && let Ok(value) = input.parse::<f64>()
        && value.is_finite()
    {
        return Some(value);
    }
    let mut normalized = String::with_capacity(input.len().min(128));
    let (mut fraction, mut exponent, mut digit) = (false, false, false);
    let mut percents = 0usize;
    for ch in input.chars().filter(|ch| !ch.is_whitespace()) {
        if ch == '%' {
            percents += 1;
            continue;
        }
        if percents != 0 {
            return None;
        }
        if group == Some(ch) {
            if fraction || exponent {
                return None;
            }
            continue;
        }
        if ch == decimal {
            if fraction || exponent {
                return None;
            }
            fraction = true;
            normalized.push('.');
        } else if ch.is_ascii_digit() {
            digit |= !exponent;
            normalized.push(ch);
        } else if matches!(ch, '+' | '-') {
            normalized.push(ch);
        } else if matches!(ch, 'E' | 'e') && digit && !exponent {
            exponent = true;
            normalized.push('e');
        } else {
            return None;
        }
    }
    if !digit {
        return None;
    }
    if percents != 0 {
        // Apply both exponents before converting to f64. Otherwise an
        // intermediate can overflow (e.g. 1e310%) or round twice.
        let (mantissa, explicit) = match normalized.find('e') {
            Some(pos) => (pos, normalized[pos + 1..].parse::<i64>().ok()?),
            None => (normalized.len(), 0),
        };
        let percent_exponent = i64::try_from(percents).ok()?.checked_mul(2)?;
        let scaled = explicit.checked_sub(percent_exponent)?;
        normalized.truncate(mantissa);
        write!(normalized, "e{scaled}").ok()?;
    }
    normalized
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
}
