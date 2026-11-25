//! Single-condition aggregates for the numeric formula evaluator.

use std::borrow::Cow;
use std::cmp::Ordering;

use regex::{Regex, RegexBuilder};

use super::{
    CellValue, DataTable, FormulaBadCell, FormulaToken as Token, aggregate_cell, eval_expression,
};

#[derive(Clone, Copy, Debug)]
pub(super) enum ConditionalAggregate {
    Count,
    Sum,
    Average,
}

#[derive(Clone, Copy)]
struct Rectangle {
    top: usize,
    left: usize,
    bottom: usize,
    right: usize,
}

fn rectangle(tokens: &[Token], pos: usize) -> Option<(Rectangle, usize)> {
    let (a, b, next) = match tokens.get(pos..)? {
        [
            Token::CellRef(r1, c1),
            Token::Range,
            Token::CellRef(r2, c2),
            ..,
        ] => ((*r1, *c1), (*r2, *c2), pos + 3),
        [Token::CellRef(row, col), ..] => ((*row, *col), (*row, *col), pos + 1),
        _ => return None,
    };
    Some((
        Rectangle {
            top: a.0.min(b.0),
            left: a.1.min(b.1),
            bottom: a.0.max(b.0),
            right: a.1.max(b.1),
        },
        next,
    ))
}

pub(super) fn evaluate(
    aggregate: ConditionalAggregate,
    tokens: &[Token],
    pos: usize,
    table: &DataTable,
    bad: &mut Option<FormulaBadCell>,
) -> Option<(f64, usize)> {
    if !matches!(tokens.get(pos), Some(Token::LParen)) {
        return None;
    }
    let (range, pos) = rectangle(tokens, pos + 1)?;
    if !matches!(tokens.get(pos), Some(Token::Separator)) {
        return None;
    }
    let (input, pos) = criterion_argument(tokens, pos + 1, table, bad)?;
    let (output, pos) = if matches!(tokens.get(pos), Some(Token::Separator))
        && !matches!(aggregate, ConditionalAggregate::Count)
    {
        rectangle(tokens, pos + 1)?
    } else {
        (range, pos)
    };
    if !matches!(tokens.get(pos), Some(Token::RParen)) {
        return None;
    }
    let condition = Condition::new(input)?;
    let mut total = 0.0;
    let mut count = 0usize;
    // Intersect with loaded rows/columns first, even for a blank criterion.
    // Sum and average ranges start at their top-left cell and use the
    // criterion rectangle's shape, not the supplied range's far corner.
    let end_row = range.bottom.saturating_add(1).min(table.row_count());
    let end_col = range.right.saturating_add(1).min(table.col_count());
    for row in range.top..end_row {
        for col in range.left..end_col {
            if !condition.matches(table.get(row, col).unwrap_or(&CellValue::Null)) {
                continue;
            }
            if matches!(aggregate, ConditionalAggregate::Count) {
                count += 1;
                continue;
            }
            if let (Some(r), Some(c)) = (
                output.top.checked_add(row - range.top),
                output.left.checked_add(col - range.left),
            ) && let Some(value) = aggregate_cell(table, r, c).filter(|value| value.is_finite())
            {
                total += value;
                count += 1;
            }
        }
    }
    let value = match aggregate {
        ConditionalAggregate::Count => count as f64,
        ConditionalAggregate::Sum => total,
        ConditionalAggregate::Average if count > 0 => total / count as f64,
        ConditionalAggregate::Average => return None,
    };
    value.is_finite().then_some((value, pos + 1))
}

enum CriterionInput<'a> {
    Number(f64),
    Boolean(bool),
    Text(Cow<'a, str>),
}

impl<'a> CriterionInput<'a> {
    fn into_text(self) -> Cow<'a, str> {
        match self {
            Self::Number(value) => value.to_string().into(),
            Self::Boolean(true) => "TRUE".into(),
            Self::Boolean(false) => "FALSE".into(),
            Self::Text(value) => value,
        }
    }
}

fn criterion_argument<'a>(
    tokens: &'a [Token],
    pos: usize,
    table: &'a DataTable,
    bad: &mut Option<FormulaBadCell>,
) -> Option<(CriterionInput<'a>, usize)> {
    let (input, mut next) = criterion_piece(tokens, pos, table, bad)?;
    if !matches!(tokens.get(next), Some(Token::Concat)) {
        return Some((input, next));
    }
    let mut text = input.into_text().into_owned();
    while matches!(tokens.get(next), Some(Token::Concat)) {
        let (piece, end) = criterion_piece(tokens, next + 1, table, bad)?;
        text.push_str(&piece.into_text());
        next = end;
    }
    Some((CriterionInput::Text(text.into()), next))
}

fn criterion_piece<'a>(
    tokens: &'a [Token],
    pos: usize,
    table: &'a DataTable,
    bad: &mut Option<FormulaBadCell>,
) -> Option<(CriterionInput<'a>, usize)> {
    let input = match tokens.get(pos..)? {
        [Token::Text(text), ..] => CriterionInput::Text(text.as_str().into()),
        [Token::Boolean(value), ..] => CriterionInput::Boolean(*value),
        [
            Token::CellRef(row, col),
            Token::Separator | Token::RParen | Token::Concat,
            ..,
        ] => match table.get(*row, *col)? {
            CellValue::Int(value) => CriterionInput::Number(*value as f64),
            CellValue::Float(value) => CriterionInput::Number(*value),
            CellValue::Null => CriterionInput::Number(0.0),
            CellValue::Bool(value) => CriterionInput::Boolean(*value),
            CellValue::String(value) | CellValue::Date(value) | CellValue::DateTime(value) => {
                CriterionInput::Text(value.as_str().into())
            }
            _ => return None,
        },
        _ => {
            let (value, end) = eval_expression(tokens, pos, table, bad)?;
            return Some((CriterionInput::Number(value), end));
        }
    };
    Some((input, pos + 1))
}

#[derive(Clone, Copy)]
enum Compare {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl Compare {
    fn split(text: &str) -> (Self, &str) {
        for (prefix, compare) in [
            (">=", Self::Ge),
            ("<=", Self::Le),
            ("<>", Self::Ne),
            ("=", Self::Eq),
            (">", Self::Gt),
            ("<", Self::Lt),
        ] {
            if let Some(value) = text.strip_prefix(prefix) {
                return (compare, value);
            }
        }
        (Self::Eq, text)
    }

    fn matches(self, order: Ordering) -> bool {
        match self {
            Self::Eq => order.is_eq(),
            Self::Ne => !order.is_eq(),
            Self::Lt => order.is_lt(),
            Self::Le => !order.is_gt(),
            Self::Gt => order.is_gt(),
            Self::Ge => !order.is_lt(),
        }
    }
}

enum Condition {
    Number(Compare, f64),
    Boolean(bool),
    Blank(bool),
    Text(Compare, String),
    Wildcard(Regex, bool),
}

impl Condition {
    fn new(input: CriterionInput<'_>) -> Option<Self> {
        Some(match input {
            CriterionInput::Number(value) => {
                return value
                    .is_finite()
                    .then_some(Self::Number(Compare::Eq, value));
            }
            CriterionInput::Boolean(value) => Self::Boolean(value),
            CriterionInput::Text(text) => {
                let (compare, operand) = Compare::split(&text);
                if operand.is_empty() {
                    match compare {
                        Compare::Eq => Self::Blank(true),
                        Compare::Ne => Self::Blank(false),
                        _ => return None,
                    }
                } else if let Ok(value) = operand.trim().parse::<f64>()
                    && value.is_finite()
                {
                    Self::Number(compare, value)
                } else if matches!(compare, Compare::Eq | Compare::Ne)
                    && operand.contains(['*', '?', '~'])
                {
                    Self::Wildcard(wildcard(operand)?, matches!(compare, Compare::Ne))
                } else {
                    Self::Text(compare, operand.to_lowercase())
                }
            }
        })
    }

    fn matches(&self, cell: &CellValue) -> bool {
        let text = match cell {
            CellValue::String(text) | CellValue::Date(text) | CellValue::DateTime(text) => {
                Some(text.as_str())
            }
            _ => None,
        };
        match self {
            Self::Number(compare, target) => {
                let number = match cell {
                    CellValue::Int(value) => Some(*value as f64),
                    CellValue::Float(value) => Some(*value),
                    CellValue::String(value) => value.trim().parse().ok(),
                    _ => None,
                };
                number
                    .filter(|value| value.is_finite())
                    .and_then(|value| value.partial_cmp(target))
                    .is_some_and(|order| compare.matches(order))
            }
            Self::Boolean(target) => matches!(cell, CellValue::Bool(value) if value == target),
            Self::Blank(target) => {
                (matches!(cell, CellValue::Null)
                    || matches!(cell, CellValue::String(text) if text.is_empty()))
                    == *target
            }
            Self::Text(compare, target) => text.is_some_and(|text| {
                let order = if text.is_ascii() && target.is_ascii() {
                    text.bytes()
                        .map(|b| b.to_ascii_lowercase())
                        .cmp(target.bytes())
                } else {
                    text.to_lowercase().cmp(target)
                };
                compare.matches(order)
            }),
            Self::Wildcard(pattern, negate) => {
                text.is_some_and(|text| pattern.is_match(text) != *negate)
            }
        }
    }
}

fn wildcard(pattern: &str) -> Option<Regex> {
    let mut regex = String::from(r"\A");
    let mut chars = pattern.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '*' => regex.push_str(".*"),
            '?' => regex.push('.'),
            '~' if matches!(chars.peek(), Some('*' | '?' | '~')) => {
                regex.push_str(&regex::escape(&chars.next()?.to_string()));
            }
            literal => regex.push_str(&regex::escape(&literal.to_string())),
        }
    }
    regex.push_str(r"\z");
    RegexBuilder::new(&regex)
        .case_insensitive(true)
        .dot_matches_new_line(true)
        .build()
        .ok()
}
