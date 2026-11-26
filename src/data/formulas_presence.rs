//! Spreadsheet completeness checks on the loaded cell rectangle.

use super::control::{arguments, unparen};
use super::criteria::{Rectangle, rectangle};
use super::{
    CellValue, DataTable, Evaluation, FormulaToken as Token, eval_complete, eval_expression,
};

#[derive(Clone, Copy, Debug)]
pub(super) enum Presence {
    CountValues,
    CountBlanks,
    IsBlank,
}

fn empty_cell(cell: Option<&CellValue>) -> bool {
    matches!(cell, None | Some(CellValue::Null))
}

fn count_cells(table: &DataTable, rect: Rectangle, blanks: bool) -> usize {
    let (rows, cols) = rect.loaded_shape(table);
    if cols == 0 {
        return 0;
    }
    let mut count = 0;
    for r in 0..rows {
        for c in 0..cols {
            let (row, col) = rect.at(r, c);
            let cell = table.get(row, col);
            let counted = if blanks {
                empty_cell(cell) || matches!(cell, Some(CellValue::String(text)) if text.is_empty())
            } else {
                !empty_cell(cell)
            };
            count += usize::from(counted);
        }
    }
    count
}

pub(super) fn evaluate(
    kind: Presence,
    tokens: &[Token],
    open: usize,
    context: &mut Evaluation<'_>,
) -> Option<(f64, usize)> {
    if !matches!(tokens.get(open), Some(Token::LParen)) {
        return None;
    }
    match kind {
        Presence::CountValues => count_values(tokens, open + 1, context),
        Presence::CountBlanks => {
            let (rect, end) = rectangle(tokens, open + 1)?;
            if !matches!(tokens.get(end), Some(Token::RParen)) {
                return None;
            }
            let count = context
                .table
                .map_or(0, |table| count_cells(table, rect, true));
            Some((count as f64, end + 1))
        }
        Presence::IsBlank => {
            let ([arg], end) = arguments(tokens, open)?;
            let blank = match unparen(arg?) {
                [Token::CellRef(row, col)] => match context.table {
                    None => false,
                    Some(table) if *row < table.row_count() && *col < table.col_count() => {
                        empty_cell(table.get(*row, *col))
                    }
                    // An unloaded reference cannot be tested for emptiness.
                    Some(_) => return None,
                },
                [Token::Text(_) | Token::Boolean(_)] => false,
                expression => {
                    eval_complete(expression, context)?;
                    false
                }
            };
            Some((f64::from(u8::from(blank)), end))
        }
    }
}

fn count_values(
    tokens: &[Token],
    mut pos: usize,
    context: &mut Evaluation<'_>,
) -> Option<(f64, usize)> {
    let mut count = 0usize;
    for _ in 0..255 {
        let (added, end) = match tokens.get(pos..)? {
            [
                Token::CellRef(..),
                Token::Range | Token::Separator | Token::RParen,
                ..,
            ] => {
                let (rect, end) = rectangle(tokens, pos)?;
                (
                    context
                        .table
                        .map_or(0, |table| count_cells(table, rect, false)),
                    end,
                )
            }
            [
                Token::Text(_) | Token::Boolean(_),
                Token::Separator | Token::RParen,
                ..,
            ] => (1, pos + 1),
            _ => {
                let (_, end) = eval_expression(tokens, pos, context)?;
                (1, end)
            }
        };
        if context.table.is_some() {
            count = count.checked_add(added)?;
        }
        match tokens.get(end)? {
            Token::Separator => pos = end + 1,
            Token::RParen => return Some((count as f64, end + 1)),
            _ => return None,
        }
    }
    None
}
