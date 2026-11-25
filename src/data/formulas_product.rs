//! Multiply corresponding cells of equal-size rectangles and sum the products.

use super::criteria::{Rectangle, rectangle};
use super::{DataTable, Evaluation, FormulaToken as Token, aggregate_cell, eval_expression};

enum Factor {
    Range(Rectangle),
    Scalar(f64),
}

impl Factor {
    fn shape(&self) -> (usize, usize) {
        match self {
            Self::Range(range) => range.shape(),
            Self::Scalar(_) => (1, 1),
        }
    }

    fn value(&self, table: &DataTable, row: usize, col: usize) -> f64 {
        match self {
            Self::Range(range) => {
                let (r, c) = range.at(row, col);
                aggregate_cell(table, r, c).unwrap_or(0.0)
            }
            Self::Scalar(value) => *value,
        }
    }
}

pub(super) fn evaluate(
    tokens: &[Token],
    open: usize,
    context: &mut Evaluation<'_>,
) -> Option<(f64, usize)> {
    const MAX_FACTORS: usize = 255;
    if !matches!(tokens.get(open), Some(Token::LParen)) {
        return None;
    }
    let mut pos = open + 1;
    let mut args = 0;
    let mut factors = Vec::new();
    let end = loop {
        if args == MAX_FACTORS {
            return None;
        }
        args += 1;
        let (factor, next) = if let Some((range, end)) = rectangle(tokens, pos)
            && matches!(tokens.get(end), Some(Token::Separator | Token::RParen))
        {
            (Factor::Range(range), end)
        } else {
            let (value, end) = eval_expression(tokens, pos, context)?;
            (Factor::Scalar(value), end)
        };
        if context.table.is_some() {
            factors.push(factor);
        }
        match tokens.get(next) {
            Some(Token::Separator) => pos = next + 1,
            Some(Token::RParen) => break next + 1,
            _ => return None,
        }
    };
    let Some(table) = context.table else {
        // Shape mismatches are runtime errors; inactive branches are parsed
        // without walking data or rejecting otherwise valid range syntax.
        return Some((0.0, end));
    };
    let shape = factors.first()?.shape();
    let (mut rows, mut cols) = shape;
    for factor in &factors {
        if factor.shape() != shape {
            return None;
        }
        if let Factor::Range(range) = factor {
            let (r, c) = range.loaded_shape(table);
            rows = rows.min(r);
            cols = cols.min(c);
        }
    }
    if cols == 0 {
        return Some((0.0, end));
    }
    let mut sum = 0.0;
    for row in 0..rows {
        for col in 0..cols {
            let mut product = 1.0;
            for factor in &factors {
                let value = factor.value(table, row, col);
                if value == 0.0 {
                    // A blank factor zeroes the term even if a preceding
                    // partial product had overflowed.
                    product = 0.0;
                    break;
                }
                product *= value;
            }
            sum += product;
            if !sum.is_finite() {
                return None;
            }
        }
    }
    Some((sum, end))
}
