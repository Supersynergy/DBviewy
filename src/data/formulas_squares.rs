//! Sum squares without losing every small term before the sum is formed.
use super::control::{argument_start, next_argument, unparen};
use super::means::{exact_range, visit_range};
use super::{Evaluation, FormulaToken as Token, eval_complete};
use crate::data::decimal::CarefulSum;

#[derive(Default)]
pub(super) struct Squares {
    ordinary: CarefulSum,
    tiny: CarefulSum,
}

impl Squares {
    pub(super) fn add(&mut self, value: f64) -> Option<()> {
        const SUBNORMAL_ROOT: f64 = f64::from_bits(512 << 52); // 2^-511
        const LIFT: f64 = f64::from_bits(1279 << 52); // 2^256
        if value != 0.0 && value.abs() < SUBNORMAL_ROOT {
            let up = value * LIFT;
            self.tiny.add(up * up);
        } else {
            let squared = value * value;
            if !squared.is_finite() {
                return None;
            }
            self.ordinary.add(squared);
        }
        Some(())
    }

    pub(super) fn value(self) -> Option<f64> {
        const UNLIFT: f64 = f64::from_bits(511 << 52); // 2^-512
        let sum = self.tiny.value().mul_add(UNLIFT, self.ordinary.value());
        sum.is_finite().then_some(sum)
    }
}

pub(super) fn evaluate(
    tokens: &[Token],
    pos: usize,
    context: &mut Evaluation<'_>,
) -> Option<(f64, usize)> {
    let mut start = argument_start(tokens, pos)?;
    let mut sum = Squares::default();
    for _ in 0..255 {
        let (tokens, end, last) = next_argument(tokens, start)?;
        let part = unparen(tokens);
        if let Some(range) = exact_range(part) {
            if let Some(table) = context.table {
                visit_range(table, range, |value| {
                    if value.is_finite() {
                        sum.add(value)
                    } else {
                        Some(())
                    }
                })?;
            }
        } else {
            let value = match part {
                [Token::Text(text)] if context.table.is_some() => text.trim().parse().ok()?,
                [Token::Text(_)] => 0.0, // Syntax-only check of a lazy branch.
                _ => eval_complete(part, context)?,
            };
            if context.table.is_some() {
                sum.add(value)?;
            }
        }
        if last {
            return if context.table.is_none() {
                Some((0.0, end))
            } else {
                Some((sum.value()?, end))
            };
        }
        start = end;
    }
    None
}
