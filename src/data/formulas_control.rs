//! Lazy control functions for the numeric formula evaluator.

use std::cmp::Ordering;

use super::{CellValue, Comparison, Evaluation, FormulaToken as Token, eval_complete};

type Arguments<'a> = [Option<&'a [Token]>; 3];

/// Split a call at its own separators, not those of nested calls.
fn arguments(tokens: &[Token], open: usize) -> Option<(Arguments<'_>, usize)> {
    if !matches!(tokens.get(open), Some(Token::LParen)) {
        return None;
    }
    let mut args = [None; 3];
    let mut start = open + 1;
    let mut slot = 0;
    let mut depth = 0usize;
    for (offset, token) in tokens.get(start..)?.iter().enumerate() {
        let pos = open + 1 + offset;
        match token {
            Token::LParen => depth += 1,
            Token::RParen if depth > 0 => depth -= 1,
            Token::Separator | Token::RParen if depth == 0 => {
                *args.get_mut(slot)? = Some(&tokens[start..pos]);
                if matches!(token, Token::RParen) {
                    return Some((args, pos + 1));
                }
                start = pos + 1;
                slot += 1;
            }
            _ => {}
        }
    }
    None
}

fn check(tokens: &[Token]) -> Option<()> {
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

/// Peel parentheses only when they wrap the whole expression.
fn unparen(mut tokens: &[Token]) -> &[Token] {
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
