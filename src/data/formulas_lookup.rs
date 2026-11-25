//! Exact and wildcard lookup in one row or column.

use regex::Regex;

use super::control::{arguments, check, unparen};
use super::criteria::{Rectangle, rectangle, wildcard};
use super::{CellValue, Evaluation, FormulaToken as Token, cell_as_f64, eval_complete};

#[derive(Clone, Copy, Debug)]
pub(super) enum ResultKind {
    Value,
    Position,
}

fn range(tokens: &[Token]) -> Option<Rectangle> {
    let (range, end) = rectangle(tokens, 0)?;
    (end == tokens.len()).then_some(range)
}

pub(super) fn evaluate(
    kind: ResultKind,
    tokens: &[Token],
    pos: usize,
    context: &mut Evaluation<'_>,
) -> Option<(f64, usize)> {
    let (key, search, output, fallback, mode, direction, end) = match kind {
        ResultKind::Value => {
            let ([key, search, output, fallback, mode, direction], end) = arguments(tokens, pos)?;
            (
                key?,
                range(search?)?,
                Some(range(output?)?),
                fallback,
                mode,
                direction,
                end,
            )
        }
        ResultKind::Position => {
            let ([key, search, mode, direction], end) = arguments(tokens, pos)?;
            (key?, range(search?)?, None, None, mode, direction, end)
        }
    };
    let fallback = optional(fallback);
    if let Some(fallback) = fallback {
        check(fallback)?;
    }
    let key = if key.is_empty() && matches!(kind, ResultKind::Value) {
        Key::Blank
    } else {
        key_value(key, context)?
    };
    let mode = option_number(mode, 0.0, context)?;
    let direction = option_number(direction, 1.0, context)?;
    let Some(table) = context.table else {
        return Some((0.0, end));
    };

    let (rows, cols) = search.shape();
    if rows != 1 && cols != 1 || output.is_some_and(|output| output.shape() != (rows, cols)) {
        return None;
    }
    let reverse = match direction {
        1.0 => false,
        -1.0 => true,
        _ => return None,
    };
    let matcher = match (mode, key) {
        (0.0, key) => Matcher::Exact(key),
        (2.0, Key::Text(pattern)) => Matcher::Wildcard(wildcard(pattern)?),
        _ => return None,
    };

    // Cap the search at loaded cells *before* reversing it. Off-sheet
    // cells are never candidates, even when looking for a blank.
    let (loaded_rows, loaded_cols) = search.loaded_shape(table);
    let len = loaded_rows * loaded_cols; // At least one dimension is <= 1.
    let at = |range: Rectangle, index| {
        if rows == 1 {
            range.at(0, index)
        } else {
            range.at(index, 0)
        }
    };
    let found = (0..len)
        .map(|offset| if reverse { len - 1 - offset } else { offset })
        .find(|&offset| {
            let (r, c) = at(search, offset);
            Key::from_cell(table.get(r, c)).is_some_and(|value| matcher.matches(value))
        });
    let value = match (found, output) {
        (Some(offset), Some(output)) => {
            let (r, c) = at(output, offset);
            if r >= table.row_count() || c >= table.col_count() {
                return None;
            }
            match cell_as_f64(table, r, c) {
                Ok(value) => value?,
                Err(bad) => {
                    if context.bad.is_none() {
                        context.bad = Some(bad);
                    }
                    return None;
                }
            }
        }
        (Some(offset), None) => (offset + 1) as f64,
        (None, _) => eval_complete(fallback?, context)?,
    };
    value.is_finite().then_some((value, end))
}

fn optional(tokens: Option<&[Token]>) -> Option<&[Token]> {
    tokens.filter(|part| !part.is_empty())
}

fn option_number(
    tokens: Option<&[Token]>,
    default: f64,
    context: &mut Evaluation<'_>,
) -> Option<f64> {
    match optional(tokens) {
        Some(tokens) => eval_complete(tokens, context),
        None => Some(default),
    }
}

#[derive(Clone, Copy)]
enum Key<'a> {
    Text(&'a str),
    Integer(i64),
    Number(f64),
    Boolean(bool),
    Blank,
}

impl<'a> Key<'a> {
    fn from_cell(cell: Option<&'a CellValue>) -> Option<Self> {
        match cell {
            Some(CellValue::Int(value)) => Some(Self::Integer(*value)),
            Some(CellValue::Float(value)) => value.is_finite().then_some(Self::Number(*value)),
            Some(
                CellValue::String(value) | CellValue::Date(value) | CellValue::DateTime(value),
            ) => Some(Self::Text(value)),
            Some(CellValue::Bool(value)) => Some(Self::Boolean(*value)),
            Some(CellValue::Null) | None => Some(Self::Blank),
            _ => None,
        }
    }

    fn equals(self, other: Self) -> bool {
        match (self, other) {
            (Self::Text(a), Self::Text(b)) => {
                a.eq_ignore_ascii_case(b)
                    || ((!a.is_ascii() || !b.is_ascii()) && a.to_lowercase() == b.to_lowercase())
            }
            (Self::Integer(a), Self::Integer(b)) => a == b,
            (Self::Number(a), Self::Number(b)) => a == b,
            (Self::Integer(a), Self::Number(b)) | (Self::Number(b), Self::Integer(a)) => {
                // Never round two different integer identifiers onto the
                // same f64. Convertible integral floats still match ints.
                b.fract() == 0.0 && b >= i64::MIN as f64 && b < i64::MAX as f64 && a == b as i64
            }
            (Self::Boolean(a), Self::Boolean(b)) => a == b,
            (Self::Blank, Self::Blank) => true,
            _ => false,
        }
    }
}

fn key_value<'a, 't: 'a>(tokens: &'a [Token], context: &mut Evaluation<'t>) -> Option<Key<'a>> {
    let tokens = unparen(tokens);
    match tokens {
        [Token::Text(text)] => Some(Key::Text(text)),
        [Token::Boolean(value)] => Some(Key::Boolean(*value)),
        [Token::CellRef(row, col)] if context.table.is_some() => {
            Key::from_cell(context.table?.get(*row, *col))
        }
        _ => {
            let value = eval_complete(tokens, context)?;
            (context.table.is_none() || value.is_finite()).then_some(Key::Number(value))
        }
    }
}

enum Matcher<'a> {
    Exact(Key<'a>),
    Wildcard(Regex),
}

impl Matcher<'_> {
    fn matches(&self, candidate: Key<'_>) -> bool {
        match self {
            Self::Exact(key) => key.equals(candidate),
            Self::Wildcard(pattern) => matches!(candidate, Key::Text(s) if pattern.is_match(s)),
        }
    }
}
