//! Text checks with numeric results, without coercing imported identifiers.

use std::{borrow::Cow, cell::RefCell, collections::VecDeque};

use regex::Regex;

use super::control::{arguments, unparen};
use super::criteria::compile_wildcard;
use super::{CellValue, Evaluation, FormulaBadCell, FormulaToken as Token, eval_complete};

#[derive(Clone, Copy, Debug)]
pub(super) enum Kind {
    Length,
    Exact,
    Find,
    Search,
}

pub(super) fn evaluate(
    kind: Kind,
    tokens: &[Token],
    pos: usize,
    context: &mut Evaluation<'_>,
) -> Option<(f64, usize)> {
    let ([first, second, third], end) = arguments(tokens, pos)?;
    let value = match kind {
        Kind::Length if second.is_none() => text(first?, context)?.chars().count() as f64,
        Kind::Exact if third.is_none() => {
            let left = text(first?, context)?;
            let right = text(second?, context)?;
            f64::from(u8::from(left == right))
        }
        Kind::Find | Kind::Search => {
            let needle = text(first?, context)?;
            let haystack = text(second?, context)?;
            let start = match third {
                Some(position) => eval_complete(position, context)?,
                None => 1.0,
            };
            if context.table.is_none() {
                // A lazy branch verifies syntax but not the existence of a
                // match, the starting position or any referenced cell.
                0.0
            } else {
                find(kind, &needle, &haystack, start)? as f64
            }
        }
        _ => return None,
    };
    Some((value, end))
}

fn find(kind: Kind, needle: &str, haystack: &str, start: f64) -> Option<usize> {
    if start.fract() != 0.0 || start < 1.0 || start > haystack.len().max(1) as f64 {
        return None;
    }
    let skipped = start as usize - 1;
    let offset = if skipped == 0 {
        0
    } else {
        haystack.char_indices().nth(skipped)?.0
    };
    let suffix = &haystack[offset..];
    let byte = if needle.is_empty() {
        0
    } else if matches!(kind, Kind::Search) {
        if suffix.is_empty() {
            return None;
        }
        // Fill shifts references in each row, but a typical pattern stays
        // constant. Share a handful of recently used patterns per thread.
        if needle.len() <= CACHEABLE_PATTERN_BYTES {
            SEARCH_PATTERNS.with_borrow_mut(|patterns| patterns.find(needle, suffix))?
        } else {
            compile_wildcard(needle, false)?.find(suffix)?.start()
        }
    } else {
        suffix.find(needle)?
    };
    Some(skipped + suffix[..byte].chars().count() + 1)
}

const SEARCH_CACHE_SLOTS: usize = 4;
const CACHEABLE_PATTERN_BYTES: usize = 256;

thread_local! {
    static SEARCH_PATTERNS: RefCell<SearchPatterns> = const {
        RefCell::new(SearchPatterns(VecDeque::new()))
    };
}

struct SearchPatterns(VecDeque<(Box<str>, Regex)>);

impl SearchPatterns {
    fn find(&mut self, pattern: &str, text: &str) -> Option<usize> {
        if let Some(index) = self
            .0
            .iter()
            .position(|(stored, _)| stored.as_ref() == pattern)
        {
            if index > 0 {
                let entry = self.0.remove(index)?;
                self.0.push_front(entry);
            }
        } else {
            let regex = compile_wildcard(pattern, false)?;
            if self.0.len() == SEARCH_CACHE_SLOTS {
                self.0.pop_back();
            }
            self.0.push_front((pattern.into(), regex));
        }
        self.0.front()?.1.find(text).map(|matched| matched.start())
    }
}

fn text<'a, 't: 'a>(tokens: &'a [Token], context: &mut Evaluation<'t>) -> Option<Cow<'a, str>> {
    let tokens = unparen(tokens);
    // Fast path for the common direct text cell or literal. In particular,
    // checking a column of long notes does not copy them into a buffer.
    if tokens.len() <= 1 {
        return scalar_text(tokens, context);
    }
    let mut depth = 0;
    let mut start = 0;
    let mut joined = String::new();
    let live = context.table.is_some();
    for (i, token) in tokens.iter().enumerate() {
        match token {
            Token::LParen => depth += 1,
            Token::RParen => depth -= 1,
            Token::Concat if depth == 0 => {
                let part = text(&tokens[start..i], context)?;
                if live {
                    joined.push_str(&part);
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    if start == 0 {
        return scalar_text(tokens, context);
    }
    let last = text(&tokens[start..], context)?;
    if live {
        joined.push_str(&last);
    }
    Some(joined.into())
}

fn boolean_text(value: bool) -> &'static str {
    if value { "TRUE" } else { "FALSE" }
}

fn number_text(number: f64) -> Option<String> {
    number.is_finite().then(|| {
        let magnitude = if number == 0.0 { 0.0 } else { number };
        magnitude.to_string()
    })
}

fn scalar_text<'a, 't: 'a>(
    tokens: &'a [Token],
    context: &mut Evaluation<'t>,
) -> Option<Cow<'a, str>> {
    match tokens {
        [Token::Text(s)] => Some(s.as_str().into()),
        [Token::Boolean(b)] => Some(boolean_text(*b).into()),
        [Token::CellRef(..)] if context.table.is_none() => Some("".into()),
        [Token::CellRef(row, col)] => {
            let value = context.table?.get(*row, *col)?;
            let text = match value {
                CellValue::Null => "".into(),
                CellValue::String(s) | CellValue::Date(s) | CellValue::DateTime(s) => {
                    s.as_str().into()
                }
                CellValue::Bool(b) => boolean_text(*b).into(),
                CellValue::Int(n) => n.to_string().into(),
                CellValue::Float(n) if n.is_finite() => number_text(*n)?.into(),
                CellValue::Time(ns) => super::super::times::format_time(*ns).into(),
                CellValue::Duration(ns) => super::super::times::format_duration(
                    *ns,
                    super::super::times::DurationStyle::Iso,
                )
                .into(),
                other => {
                    if context.bad.is_none() {
                        context.bad = Some(FormulaBadCell {
                            row: *row,
                            col: *col,
                            content: match other {
                                CellValue::Binary(_) => "binary".into(),
                                CellValue::Nested(_) => "structured data".into(),
                                _ => other.to_string(),
                            },
                        });
                    }
                    return None;
                }
            };
            Some(text)
        }
        _ => {
            let number = eval_complete(tokens, context)?;
            if context.table.is_none() {
                Some("".into())
            } else {
                number_text(number).map(Cow::Owned)
            }
        }
    }
}

#[cfg(test)]
#[path = "formulas_text_tests.rs"]
mod tests;
