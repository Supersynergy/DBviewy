//! Discount periodic or dated cash flows without allocating a copy of a range.

use super::control::{arguments, unparen};
use super::criteria::{Rectangle, rectangle};
use super::{
    CellValue, DataTable, Evaluation, FormulaBadCell, FormulaToken as Token, date, eval_complete,
    eval_expression, snippet,
};

/// Keep a small result when large payments nearly cancel. Discard an
/// invalid discount factor or unrepresentable result, never a cash flow.
#[derive(Default)]
struct PresentValue {
    total: f64,
    lost: f64,
}

impl PresentValue {
    fn add(&mut self, amount: f64, exponent: f64) -> Option<()> {
        if !amount.is_finite() {
            return None;
        }
        if amount == 0.0 {
            return Some(());
        }
        let factor = exponent.exp();
        let discounted = if factor.is_normal() {
            amount * factor
        } else {
            // An underflowed or overflowed factor need not imply the
            // product is out of range (for example, a very large payment).
            (amount.abs().ln() + exponent).exp().copysign(amount)
        };
        let next = self.total + discounted;
        if !next.is_finite() {
            return None;
        }
        self.lost += if self.total.abs() >= discounted.abs() {
            (self.total - next) + discounted
        } else {
            (discounted - next) + self.total
        };
        self.total = next;
        Some(())
    }

    fn finish(self) -> Option<f64> {
        let result = self.total + self.lost;
        result.is_finite().then_some(result)
    }
}

fn log_discount_rate(rate: f64) -> Option<f64> {
    (rate > -1.0 && rate.is_finite()).then(|| rate.ln_1p())
}

fn numeric_amount(value: &CellValue) -> Option<f64> {
    match value {
        CellValue::Int(n) => Some(*n as f64),
        CellValue::Float(n) => Some(*n),
        _ => None,
    }
}

pub(super) fn periodic(
    tokens: &[Token],
    open: usize,
    context: &mut Evaluation<'_>,
) -> Option<(f64, usize)> {
    if !matches!(tokens.get(open), Some(Token::LParen)) {
        return None;
    }
    let (rate, end) = eval_expression(tokens, open + 1, context)?;
    if !matches!(tokens.get(end), Some(Token::Separator)) {
        return None;
    }
    let table = context.table;
    let log_rate = if table.is_some() {
        log_discount_rate(rate)?
    } else {
        0.0
    };
    let mut past_periods = 0u64;
    let mut total = PresentValue::default();
    let mut record = |amount| {
        // The first amount is at the end of period one. A real zero
        // advances the period, but non-numeric cells do not.
        past_periods = past_periods.checked_add(1)?;
        total.add(amount, -(past_periods as f64) * log_rate)
    };

    let mut pos = end + 1;
    for _ in 0..254 {
        let next = if let Some((range, end)) = rectangle(tokens, pos)
            && matches!(tokens.get(end), Some(Token::Separator | Token::RParen))
        {
            if let Some(table) = table {
                let (rows, cols) = range.loaded_shape(table);
                if cols != 0 {
                    for r in 0..rows {
                        for c in 0..cols {
                            let (row, col) = range.at(r, c);
                            if let Some(amount) = table.get(row, col).and_then(numeric_amount) {
                                record(amount)?;
                            }
                        }
                    }
                }
            }
            end
        } else if matches!(
            tokens.get(pos..),
            Some([
                Token::Text(_) | Token::Boolean(_),
                Token::Separator | Token::RParen,
                ..
            ])
        ) {
            pos + 1
        } else {
            let (amount, end) = eval_expression(tokens, pos, context)?;
            if table.is_some() {
                record(amount)?;
            }
            end
        };
        match tokens.get(next)? {
            Token::RParen => return Some((total.finish()?, next + 1)),
            Token::Separator => pos = next + 1,
            _ => return None,
        }
    }
    None
}

fn vector(tokens: &[Token]) -> Option<Rectangle> {
    let tokens = unparen(tokens);
    let (range, end) = rectangle(tokens, 0)?;
    (end == tokens.len()).then_some(range)
}

fn vector_len(range: Rectangle, table: &DataTable) -> Option<usize> {
    let (rows, cols) = range.shape();
    ((rows == 1 || cols == 1) && (rows, cols) == range.loaded_shape(table))
        .then_some(rows.max(cols))
}

fn at(range: Rectangle, offset: usize) -> (usize, usize) {
    if range.shape().0 == 1 {
        range.at(0, offset)
    } else {
        range.at(offset, 0)
    }
}

pub(super) fn dated(
    tokens: &[Token],
    open: usize,
    context: &mut Evaluation<'_>,
) -> Option<(f64, usize)> {
    let ([rate, amounts, dates], end) = arguments(tokens, open)?;
    let rate = eval_complete(rate?, context)?;
    let amounts = vector(amounts?)?;
    let dates = vector(dates?)?;
    let Some(table) = context.table else {
        return Some((0.0, end));
    };
    let log_rate = log_discount_rate(rate)?;
    let len = vector_len(amounts, table)?;
    if len != vector_len(dates, table)? || len < 2 {
        return None;
    }

    let (start_row, start_col) = at(dates, 0);
    let start = date::date_cell(context, start_row, start_col)?;
    let mut positive = false;
    let mut negative = false;
    let mut total = PresentValue::default();
    for i in 0..len {
        let (r, c) = at(amounts, i);
        let cell = table.get(r, c).unwrap_or(&CellValue::Null);
        let Some(amount) = numeric_amount(cell).filter(|n| n.is_finite()) else {
            if context.bad.is_none() {
                context.bad = Some(FormulaBadCell {
                    row: r,
                    col: c,
                    content: snippet(&cell.to_string()),
                });
            }
            return None;
        };
        positive |= amount > 0.0;
        negative |= amount < 0.0;
        let days = if i == 0 {
            0
        } else {
            let (r, c) = at(dates, i);
            (date::date_cell(context, r, c)? - start).num_days()
        };
        if days < 0 {
            return None;
        }
        total.add(amount, -(days as f64 / 365.0) * log_rate)?;
    }
    if !positive || !negative {
        return None;
    }
    Some((total.finish()?, end))
}
