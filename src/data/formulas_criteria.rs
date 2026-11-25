//! Conditional aggregates for the numeric formula evaluator.

use std::borrow::Cow;

use regex::{Regex, RegexBuilder};

use super::{
    CellValue, Comparison as Compare, DataTable, Evaluation, FormulaToken as Token, aggregate_cell,
    eval_expression,
};

#[derive(Clone, Copy, Debug)]
pub(super) enum ConditionalAggregate {
    Count,
    Sum,
    Average,
}

#[derive(Clone, Copy)]
pub(super) struct Rectangle {
    top: usize,
    left: usize,
    bottom: usize,
    right: usize,
}

impl Rectangle {
    pub(super) fn at(self, row: usize, col: usize) -> (usize, usize) {
        (self.top + row, self.left + col)
    }

    pub(super) fn shape(self) -> (usize, usize) {
        (self.bottom - self.top + 1, self.right - self.left + 1)
    }

    pub(super) fn loaded_shape(self, table: &DataTable) -> (usize, usize) {
        let (rows, cols) = self.shape();
        (
            rows.min(table.row_count().saturating_sub(self.top)),
            cols.min(table.col_count().saturating_sub(self.left)),
        )
    }
}

pub(super) fn rectangle(tokens: &[Token], pos: usize) -> Option<(Rectangle, usize)> {
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
    context: &mut Evaluation<'_>,
) -> Option<(f64, usize)> {
    if !matches!(tokens.get(pos), Some(Token::LParen)) {
        return None;
    }
    let (range, pos) = rectangle(tokens, pos + 1)?;
    if !matches!(tokens.get(pos), Some(Token::Separator)) {
        return None;
    }
    let (input, pos) = criterion_argument(tokens, pos + 1, context)?;
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
    let Some(table) = context.table else {
        return Some((0.0, pos + 1));
    };
    let condition = Condition::new(input)?;
    let output = (!matches!(aggregate, ConditionalAggregate::Count)).then_some(output);
    // The singular forms use the criterion rectangle's shape even when
    // the value range specifies a different far corner.
    let value = aggregate_matching(
        aggregate,
        table,
        range.loaded_shape(table),
        output,
        |r, c| {
            condition.matches(
                table
                    .get(range.top + r, range.left + c)
                    .unwrap_or(&CellValue::Null),
            )
        },
    )?;
    Some((value, pos + 1))
}

/// Plural forms pair equal-size rectangles by relative position. Sum and
/// average take the result rectangle first, unlike their singular forms.
pub(super) fn evaluate_many(
    aggregate: ConditionalAggregate,
    tokens: &[Token],
    open: usize,
    context: &mut Evaluation<'_>,
) -> Option<(f64, usize)> {
    const MAX_PAIRS: usize = 127;
    if !matches!(tokens.get(open), Some(Token::LParen)) {
        return None;
    }
    let (output, mut pos) = if matches!(aggregate, ConditionalAggregate::Count) {
        (None, open + 1)
    } else {
        let (range, next) = rectangle(tokens, open + 1)?;
        if !matches!(tokens.get(next), Some(Token::Separator)) {
            return None;
        }
        (Some(range), next + 1)
    };
    let mut pairs = 0;
    let mut criteria = Vec::new();
    let mut shape = output.map(Rectangle::shape);
    let end = loop {
        if pairs == MAX_PAIRS {
            return None;
        }
        pairs += 1;
        let (range, next) = rectangle(tokens, pos)?;
        if !matches!(tokens.get(next), Some(Token::Separator)) {
            return None;
        }
        let (input, next) = criterion_argument(tokens, next + 1, context)?;
        if context.table.is_some() {
            let range_shape = range.shape();
            if shape
                .replace(range_shape)
                .is_some_and(|shape| shape != range_shape)
            {
                return None;
            }
            criteria.push((range, Condition::new(input)?));
        }
        match tokens.get(next) {
            Some(Token::RParen) => break next + 1,
            Some(Token::Separator) => pos = next + 1,
            _ => return None,
        }
    };
    let Some(table) = context.table else {
        // No data lookup, regex compilation or shape errors when checking
        // syntax alone. IFERROR can handle a valid call with unequal ranges.
        return Some((0.0, end));
    };
    // Intersect loaded offsets across every rectangle before scanning.
    // Unloaded cells must not turn into matching blank cells.
    let mut bounds = output.map(|range| range.loaded_shape(table));
    for (range, _) in &criteria {
        let (rows, cols) = range.loaded_shape(table);
        bounds = Some(bounds.map_or((rows, cols), |(r, c)| (r.min(rows), c.min(cols))));
    }
    let value = aggregate_matching(aggregate, table, bounds?, output, |r, c| {
        criteria.iter().all(|(range, condition)| {
            let cell = table
                .get(range.top + r, range.left + c)
                .unwrap_or(&CellValue::Null);
            condition.matches(cell)
        })
    })?;
    Some((value, end))
}

fn aggregate_matching(
    aggregate: ConditionalAggregate,
    table: &DataTable,
    (rows, cols): (usize, usize),
    output: Option<Rectangle>,
    mut matches: impl FnMut(usize, usize) -> bool,
) -> Option<f64> {
    let mut total = 0.0;
    let mut count = 0usize;
    if cols > 0 {
        for r in 0..rows {
            for c in 0..cols {
                if !matches(r, c) {
                    continue;
                }
                if let Some(output) = output {
                    let value = output
                        .top
                        .checked_add(r)
                        .zip(output.left.checked_add(c))
                        .and_then(|(row, col)| aggregate_cell(table, row, col))
                        .filter(|value| value.is_finite());
                    let Some(value) = value else { continue };
                    total += value;
                }
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
    value.is_finite().then_some(value)
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

fn criterion_argument<'a, 't: 'a>(
    tokens: &'a [Token],
    pos: usize,
    context: &mut Evaluation<'t>,
) -> Option<(CriterionInput<'a>, usize)> {
    let (input, mut next) = criterion_piece(tokens, pos, context)?;
    if !matches!(tokens.get(next), Some(Token::Concat)) {
        return Some((input, next));
    }
    let mut text = context
        .table
        .is_some()
        .then(|| input.into_text().into_owned());
    while matches!(tokens.get(next), Some(Token::Concat)) {
        let (piece, end) = criterion_piece(tokens, next + 1, context)?;
        if let Some(text) = &mut text {
            text.push_str(&piece.into_text());
        }
        next = end;
    }
    Some((
        text.map_or(CriterionInput::Number(0.0), |text| {
            CriterionInput::Text(text.into())
        }),
        next,
    ))
}

fn criterion_piece<'a, 't: 'a>(
    tokens: &'a [Token],
    pos: usize,
    context: &mut Evaluation<'t>,
) -> Option<(CriterionInput<'a>, usize)> {
    let input = match tokens.get(pos..)? {
        [Token::Text(text), ..] => CriterionInput::Text(text.as_str().into()),
        [Token::Boolean(value), ..] => CriterionInput::Boolean(*value),
        [
            Token::CellRef(..),
            Token::Separator | Token::RParen | Token::Concat,
            ..,
        ] if context.table.is_none() => CriterionInput::Number(0.0),
        [
            Token::CellRef(row, col),
            Token::Separator | Token::RParen | Token::Concat,
            ..,
        ] => match context.table?.get(*row, *col)? {
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
            let (value, end) = eval_expression(tokens, pos, context)?;
            return Some((CriterionInput::Number(value), end));
        }
    };
    Some((input, pos + 1))
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

pub(super) fn wildcard(pattern: &str) -> Option<Regex> {
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
