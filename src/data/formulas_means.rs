//! Geometric, harmonic and symmetric trimmed means on loaded worksheet cells.

use super::control::{argument_start, arguments, next_argument, unparen};
use super::criteria::{Rectangle, rectangle};
use super::{CellValue, DataTable, Evaluation, FormulaToken as Token, eval_complete};
use crate::data::decimal::{CarefulSum, decimal_parts};

#[derive(Clone, Copy, Debug)]
pub(super) enum Kind {
    Geometric,
    Harmonic,
    Trimmed,
}

pub(super) fn exact_range(tokens: &[Token]) -> Option<Rectangle> {
    let (range, end) = rectangle(tokens, 0)?;
    (end == tokens.len()).then_some(range)
}

/// A quoted identification code isn't a measurement. Decimal columns can
/// keep large numbers in strings to avoid rounding them at ingestion.
pub(super) fn visit_range(
    table: &DataTable,
    range: Rectangle,
    mut take: impl FnMut(f64) -> Option<()>,
) -> Option<()> {
    let (rows, cols) = range.loaded_shape(table);
    if rows == 0 || cols == 0 {
        return Some(());
    }
    let decimals: Vec<_> = (0..cols)
        .map(|c| {
            let (_, col) = range.at(0, c);
            decimal_parts(&table.columns[col].data_type).is_some()
        })
        .collect();
    for row in 0..rows {
        for (col, &decimal) in decimals.iter().enumerate() {
            let (r, c) = range.at(row, col);
            let value = match table.get(r, c) {
                Some(CellValue::Int(n)) => *n as f64,
                Some(CellValue::Float(n)) => *n,
                Some(CellValue::String(text)) if decimal => {
                    // A string edit in a declared decimal column is still
                    // usable when it holds a valid number.
                    let Ok(n) = text.trim().parse::<f64>() else {
                        continue;
                    };
                    n
                }
                _ => continue,
            };
            take(value)?;
        }
    }
    Some(())
}

pub(super) fn evaluate(
    kind: Kind,
    tokens: &[Token],
    pos: usize,
    context: &mut Evaluation<'_>,
) -> Option<(f64, usize)> {
    if matches!(kind, Kind::Trimmed) {
        return evaluate_trimmed(tokens, pos, context);
    }
    let mut start = argument_start(tokens, pos)?;
    let mut mean = PositiveMean::new(matches!(kind, Kind::Harmonic));
    for _ in 0..255 {
        let (tokens, end, last) = next_argument(tokens, start)?;
        let part = unparen(tokens);
        if let Some(range) = exact_range(part) {
            if let Some(table) = context.table {
                visit_range(table, range, |n| mean.add(n))?;
            }
        } else {
            let n = match part {
                [Token::Text(text)] if context.table.is_some() => text.trim().parse().ok()?,
                [Token::Text(_)] => 0.0, // Syntax-only check of a lazy branch.
                _ => eval_complete(part, context)?,
            };
            if context.table.is_some() {
                mean.add(n)?;
            }
        }
        if last {
            return if context.table.is_none() {
                Some((0.0, end))
            } else {
                Some((mean.result()?, end))
            };
        }
        start = end;
    }
    None
}

struct PositiveMean {
    harmonic: bool,
    sum: CarefulSum,
    minimum: f64,
    maximum: f64,
    count: usize,
}

impl PositiveMean {
    fn new(harmonic: bool) -> Self {
        Self {
            harmonic,
            sum: CarefulSum::default(),
            minimum: f64::INFINITY,
            maximum: 0.0,
            count: 0,
        }
    }

    fn add(&mut self, n: f64) -> Option<()> {
        if n <= 0.0 || !n.is_finite() {
            return None;
        }
        if n < self.minimum {
            if self.harmonic {
                // Sum min/x rather than 1/x to stay finite for subnormals.
                self.sum.scale(n / self.minimum);
            }
            self.minimum = n;
        }
        self.maximum = self.maximum.max(n);
        self.sum.add(if self.harmonic {
            self.minimum / n
        } else {
            n.log2()
        });
        self.count += 1;
        Some(())
    }

    fn result(self) -> Option<f64> {
        if self.count == 0 {
            return None;
        }
        if self.minimum == self.maximum {
            return Some(self.minimum);
        }
        let mean = self.sum.value() / self.count as f64;
        let value = if self.harmonic {
            self.minimum / mean
        } else {
            mean.exp2()
        };
        // The true mean is bounded by the inputs. In particular log2 can
        // round to 1024 for numbers just below f64's maximum.
        Some(value.clamp(self.minimum, self.maximum))
    }
}

fn evaluate_trimmed(
    tokens: &[Token],
    start: usize,
    context: &mut Evaluation<'_>,
) -> Option<(f64, usize)> {
    let ([range, fraction], end) = arguments(tokens, start)?;
    let range = exact_range(unparen(range?))?;
    let fraction = eval_complete(fraction?, context)?;
    let Some(table) = context.table else {
        return Some((0.0, end));
    };
    if !(0.0..=1.0).contains(&fraction) {
        return None;
    }
    let mut values = Vec::new();
    visit_range(table, range, |number| {
        number.is_finite().then(|| values.push(number))
    })?;
    Some((trimmed(values, fraction)?, end))
}

fn trimmed(mut values: Vec<f64>, fraction: f64) -> Option<f64> {
    let trim = (values.len() as f64 * fraction / 2.0).floor() as usize;
    let count = values.len().checked_sub(trim * 2)?;
    if count == 0 {
        return None;
    }
    if trim > 0 {
        // Find the two cutoffs in linear time. The retained order doesn't
        // matter to the mean, so don't sort the entire range.
        values.select_nth_unstable_by(trim, f64::total_cmp);
        values[trim..].select_nth_unstable_by(count - 1, f64::total_cmp);
    }
    let kept = &values[trim..trim + count];
    let mut sum = CarefulSum::default();
    for &number in kept {
        sum.add(number);
    }
    let ordinary = sum.value() / count as f64;
    if ordinary.is_finite() {
        return Some(ordinary);
    }
    // Keep the mean finite even when the unnormalised sum overflows.
    let scale = kept.iter().fold(0.0_f64, |max, n| max.max(n.abs()));
    let mut sum = CarefulSum::default();
    for &n in kept {
        sum.add(n / scale);
    }
    Some((sum.value() / count as f64).clamp(-1.0, 1.0) * scale)
}
