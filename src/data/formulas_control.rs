//! Logical and lazy control functions for the numeric formula evaluator.

use std::cmp::Ordering;

use super::criteria::rectangle;
use super::{CellValue, Comparison, Evaluation, FormulaToken as Token, eval_complete};

type Arguments<'a, const N: usize> = [Option<&'a [Token]>; N];

/// Take one argument and advance beyond its separator or final parenthesis.
/// Nested calls stay together, and even an empty argument has a position.
pub(super) fn next_argument(tokens: &[Token], start: usize) -> Option<(&[Token], usize, bool)> {
    let mut depth = 0usize;
    for (offset, token) in tokens.get(start..)?.iter().enumerate() {
        let pos = start + offset;
        match token {
            Token::LParen => depth += 1,
            Token::RParen if depth > 0 => depth -= 1,
            Token::Separator | Token::RParen if depth == 0 => {
                return Some((&tokens[start..pos], pos + 1, matches!(token, Token::RParen)));
            }
            _ => {}
        }
    }
    None
}

pub(super) fn argument_start(tokens: &[Token], open: usize) -> Option<usize> {
    matches!(tokens.get(open), Some(Token::LParen)).then_some(open + 1)
}

pub(super) fn arguments<const N: usize>(
    tokens: &[Token],
    open: usize,
) -> Option<(Arguments<'_, N>, usize)> {
    let mut start = argument_start(tokens, open)?;
    let mut args = [None; N];
    for slot in &mut args {
        let (arg, next, last) = next_argument(tokens, start)?;
        *slot = Some(arg);
        if last {
            return Some((args, next));
        }
        start = next;
    }
    None
}

pub(super) fn check(tokens: &[Token]) -> Option<()> {
    eval_complete(tokens, &mut Evaluation::default()).map(|_| ())
}

pub(super) fn evaluate_if(
    tokens: &[Token],
    pos: usize,
    context: &mut Evaluation<'_>,
) -> Option<(f64, usize)> {
    let ([test, yes, no], next) = arguments(tokens, pos)?;
    let yes = yes?;
    let no = no.filter(|part| !part.is_empty());
    let selected = condition(test?, context)?;
    if context.table.is_none() {
        eval_complete(yes, context)?;
        if let Some(no) = no {
            eval_complete(no, context)?;
        }
        return Some((0.0, next));
    }
    let (active, inactive) = if selected {
        (Some(yes), no)
    } else {
        (no, Some(yes))
    };
    if let Some(inactive) = inactive {
        check(inactive)?;
    }
    let value = match active {
        Some(active) => eval_complete(active, context)?,
        None => 0.0,
    };
    value.is_finite().then_some((value, next))
}

pub(super) fn evaluate_if_error(
    tokens: &[Token],
    pos: usize,
    context: &mut Evaluation<'_>,
) -> Option<(f64, usize)> {
    let ([primary, fallback, extra], next) = arguments(tokens, pos)?;
    let (primary, fallback) = (primary?, fallback?);
    if extra.is_some() {
        return None;
    }
    if context.table.is_none() {
        eval_complete(primary, context)?;
        eval_complete(fallback, context)?;
        return Some((0.0, next));
    }

    // The primary expression's diagnostics belong to the handled error,
    // not to the result. Only a failing active fallback can report its cell.
    let mut probe = Evaluation {
        table: context.table,
        bad: None,
    };
    if let Some(value) = eval_complete(primary, &mut probe).filter(|v| v.is_finite()) {
        check(fallback)?;
        return Some((value, next));
    }
    // None can also mean malformed syntax (e.g. 1/0+). Never turn that
    // into a plausible fallback number.
    check(primary)?;
    let value = eval_complete(fallback, context)?;
    value.is_finite().then_some((value, next))
}

pub(super) fn evaluate_ifs(
    tokens: &[Token],
    open: usize,
    context: &mut Evaluation<'_>,
) -> Option<(f64, usize)> {
    const MAX_PAIRS: usize = 127;
    let mut result = None;
    let mut start = argument_start(tokens, open)?;
    for _ in 0..MAX_PAIRS {
        let (test, next, no_value) = next_argument(tokens, start)?;
        if no_value {
            return None;
        }
        let (value, end, last) = next_argument(tokens, next)?;
        let selected = if context.table.is_some() && result.is_none() {
            condition(test, context)?
        } else {
            condition(test, &mut Evaluation::default())?;
            false
        };
        if selected {
            let value = if value.is_empty() {
                0.0
            } else {
                eval_complete(value, context)?
            };
            if !value.is_finite() {
                return None;
            }
            result = Some(value);
        } else if !value.is_empty() {
            check(value)?;
        }

        if last {
            return if context.table.is_none() {
                Some((0.0, end))
            } else {
                Some((result?, end))
            };
        }
        start = end;
    }
    None
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Logical {
    And,
    Or,
    Xor,
    Not,
}

impl Logical {
    fn max_args(self) -> usize {
        match self {
            Self::Not => 1,
            Self::Xor => 254,
            Self::And | Self::Or => 255,
        }
    }

    fn combine(self, previous: bool, next: bool) -> bool {
        match self {
            Self::And => previous & next,
            Self::Or => previous | next,
            Self::Xor => previous ^ next,
            Self::Not => !next,
        }
    }
}

pub(super) fn evaluate_logical(
    op: Logical,
    tokens: &[Token],
    open: usize,
    context: &mut Evaluation<'_>,
) -> Option<(f64, usize)> {
    let mut seen = false;
    let mut result = matches!(op, Logical::And);
    let mut start = argument_start(tokens, open)?;
    for _ in 0..op.max_args() {
        let (part, end, last) = next_argument(tokens, start)?;
        let part = unparen(part);
        if !matches!(op, Logical::Not)
            && let Some((range, consumed)) = rectangle(part, 0)
            && consumed == part.len()
        {
            if let Some(table) = context.table {
                let (rows, cols) = range.loaded_shape(table);
                // References ignore blanks and text; expressions use the
                // regular predicate rules. Never scan unloaded rows or fill
                // the entire rectangle with synthetic missing values.
                if cols > 0 {
                    for r in 0..rows {
                        for c in 0..cols {
                            let (row, col) = range.at(r, c);
                            let truth = match table.get(row, col) {
                                Some(CellValue::Bool(value)) => *value,
                                Some(CellValue::Int(value)) => *value != 0,
                                Some(CellValue::Float(value)) => {
                                    if !value.is_finite() {
                                        return None;
                                    }
                                    *value != 0.0
                                }
                                _ => continue,
                            };
                            result = op.combine(result, truth);
                            seen = true;
                        }
                    }
                }
            }
        } else {
            result = op.combine(result, condition(part, context)?);
            seen = true;
        }
        if last {
            return if context.table.is_none() {
                Some((0.0, end))
            } else {
                seen.then_some((f64::from(u8::from(result)), end))
            };
        }
        start = end;
    }
    None
}

/// Peel parentheses only when they wrap the whole expression.
pub(super) fn unparen(mut tokens: &[Token]) -> &[Token] {
    while let [Token::LParen, .., Token::RParen] = tokens {
        let mut depth = 0usize;
        let end = tokens.iter().position(|token| {
            match token {
                Token::LParen => depth += 1,
                Token::RParen => depth -= 1,
                _ => {}
            }
            depth == 0
        });
        if end != Some(tokens.len() - 1) {
            break;
        }
        tokens = &tokens[1..tokens.len() - 1];
    }
    tokens
}

fn condition(tokens: &[Token], context: &mut Evaluation<'_>) -> Option<bool> {
    let tokens = unparen(tokens);
    let mut depth = 0usize;
    let mut compare = None;
    for (i, token) in tokens.iter().enumerate() {
        match token {
            Token::LParen => depth += 1,
            Token::RParen => depth -= 1,
            Token::Compare(op) if depth == 0 => {
                if compare.replace((i, *op)).is_some() {
                    return None;
                }
            }
            _ => {}
        }
    }
    if let Some((i, op)) = compare {
        let left = operand(&tokens[..i], context)?;
        let right = operand(&tokens[i + 1..], context)?;
        if context.table.is_none() {
            return Some(false);
        }
        return compare_values(left, right, op);
    }
    let value = operand(tokens, context)?;
    if context.table.is_none() {
        return Some(false);
    }
    match value {
        Operand::Number(value) => Some(value != 0.0),
        Operand::Blank => Some(false),
        Operand::Text(_) => None,
    }
}

#[derive(Clone, Copy)]
enum Operand<'a> {
    Number(f64),
    Text(&'a str),
    Blank,
}

fn operand<'a, 't: 'a>(tokens: &'a [Token], context: &mut Evaluation<'t>) -> Option<Operand<'a>> {
    let tokens = unparen(tokens);
    match tokens {
        [Token::Text(text)] => return Some(Operand::Text(text)),
        [Token::CellRef(row, col)] => {
            if let Some(table) = context.table {
                match table.get(*row, *col) {
                    Some(
                        CellValue::String(text) | CellValue::Date(text) | CellValue::DateTime(text),
                    ) => {
                        return Some(Operand::Text(text));
                    }
                    Some(CellValue::Bool(value)) => {
                        return Some(Operand::Number(f64::from(u8::from(*value))));
                    }
                    Some(CellValue::Null) | None => return Some(Operand::Blank),
                    _ => {}
                }
            }
        }
        _ => {}
    }
    let value = eval_complete(tokens, context)?;
    (context.table.is_none() || value.is_finite()).then_some(Operand::Number(value))
}

fn compare_values(left: Operand<'_>, right: Operand<'_>, op: Comparison) -> Option<bool> {
    use Operand::{Blank, Number, Text};
    let order = match (left, right) {
        (Text(a), Text(b)) => Some(text_order(a, b)),
        (Text(a), Blank) => Some(text_order(a, "")),
        (Blank, Text(b)) => Some(text_order("", b)),
        (Number(a), Text(b)) => numeric_text(b).and_then(|b| a.partial_cmp(&b)),
        (Text(a), Number(b)) => numeric_text(a).and_then(|a| a.partial_cmp(&b)),
        (Number(a), Number(b)) => a.partial_cmp(&b),
        (Blank, Number(b)) => 0.0f64.partial_cmp(&b),
        (Number(a), Blank) => a.partial_cmp(&0.0),
        (Blank, Blank) => Some(Ordering::Equal),
    };
    match (order, op) {
        (Some(order), _) => Some(op.matches(order)),
        (None, Comparison::Eq) => Some(false),
        (None, Comparison::Ne) => Some(true),
        (None, _) => None,
    }
}

fn numeric_text(text: &str) -> Option<f64> {
    text.trim().parse::<f64>().ok().filter(|n| n.is_finite())
}

fn text_order(a: &str, b: &str) -> Ordering {
    if a.is_ascii() && b.is_ascii() {
        a.bytes()
            .map(|byte| byte.to_ascii_lowercase())
            .cmp(b.bytes().map(|byte| byte.to_ascii_lowercase()))
    } else {
        a.to_lowercase().cmp(&b.to_lowercase())
    }
}
