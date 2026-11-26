//! Inclusive and exclusive quantiles over the loaded part of a single range.

use super::control::{arguments, unparen};
use super::criteria::rectangle;
use super::{Evaluation, FormulaToken as Token, aggregate_cell, eval_complete};

#[derive(Clone, Copy, Debug)]
pub(super) enum Kind {
    PercentileInc,
    PercentileExc,
    QuartileInc,
    QuartileExc,
}

pub(super) fn evaluate(
    kind: Kind,
    tokens: &[Token],
    pos: usize,
    context: &mut Evaluation<'_>,
) -> Option<(f64, usize)> {
    let ([range, k], end) = arguments(tokens, pos)?;
    let range = unparen(range?);
    let (rect, consumed) = rectangle(range, 0)?;
    if consumed != range.len() {
        return None;
    }
    let k = eval_complete(k?, context)?;
    let Some(table) = context.table else {
        return Some((0.0, end));
    };
    if !k.is_finite() {
        return None;
    }
    let p = match kind {
        Kind::QuartileInc | Kind::QuartileExc => k.trunc() / 4.0,
        _ => k,
    };
    let exclusive = matches!(kind, Kind::PercentileExc | Kind::QuartileExc);
    if !(0.0..=1.0).contains(&p) || (exclusive && (p == 0.0 || p == 1.0)) {
        return None;
    }
    let (rows, cols) = rect.loaded_shape(table);
    let mut values = Vec::new();
    for row in 0..rows {
        for col in 0..cols {
            let (r, c) = rect.at(row, col);
            if let Some(n) = aggregate_cell(table, r, c).filter(|n| n.is_finite()) {
                values.push(n);
            }
        }
    }
    let last = values.len().checked_sub(1)? as f64;
    let rank = if exclusive {
        let denominator = values.len() as f64 + 1.0;
        if !(1.0 / denominator..=values.len() as f64 / denominator).contains(&p) {
            return None;
        }
        // Clamp the roundoff from converting the valid probability to a rank.
        (p * denominator - 1.0).clamp(0.0, last)
    } else {
        p * last
    };
    Some((interpolate(&mut values, rank), end))
}

fn interpolate(values: &mut [f64], rank: f64) -> f64 {
    let high = rank.ceil();
    let (before, upper, _) = values.select_nth_unstable_by(high as usize, f64::total_cmp);
    if rank == high {
        return *upper;
    }
    // The lower rank is the largest value in the left partition. Neither
    // side needs to be sorted to interpolate between adjacent ranks.
    let lower = before.iter().copied().max_by(f64::total_cmp).unwrap();
    let weight = rank.fract();
    if lower.is_sign_negative() == upper.is_sign_negative() {
        (*upper - lower).mul_add(weight, lower).clamp(lower, *upper)
    } else {
        // Subtracting a large negative endpoint from a large positive one
        // could overflow even though their weighted average is finite.
        lower * (1.0 - weight) + *upper * weight
    }
}
