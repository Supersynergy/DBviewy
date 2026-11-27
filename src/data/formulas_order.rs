//! Order and positions of finite numbers in the loaded part of a range.

use super::control::{arguments, unparen};
use super::criteria::{Rectangle, rectangle};
use super::{
    DataTable, Evaluation, FormulaToken as Token, aggregate_cell, eval_complete, rank_math,
};

fn exact_range(tokens: &[Token]) -> Option<Rectangle> {
    let tokens = unparen(tokens);
    let (range, rest) = rectangle(tokens, 0)?;
    (rest == tokens.len()).then_some(range)
}

fn numbers(range: Rectangle, table: &DataTable) -> impl Iterator<Item = f64> + '_ {
    let (rows, cols) = range.loaded_shape(table);
    (0..rows)
        .flat_map(move |r| (0..cols).map(move |c| range.at(r, c)))
        .filter_map(|(r, c)| aggregate_cell(table, r, c).filter(|n| n.is_finite()))
}

pub(super) fn rank(
    kind: rank_math::Kind,
    tokens: &[Token],
    pos: usize,
    context: &mut Evaluation<'_>,
) -> Option<(f64, usize)> {
    let ([number, range, order], end) = arguments(tokens, pos)?;
    let number = eval_complete(number?, context)?;
    let range = exact_range(range?)?;
    let order = match order {
        Some(tokens) => eval_complete(tokens, context)?,
        None => 0.0,
    };
    let Some(table) = context.table else {
        return Some((0.0, end));
    };
    let asc = rank_math::ascending(order)?;
    if !number.is_finite() {
        return None;
    }
    let mut ahead = 0;
    let mut tied = 0;
    for value in numbers(range, table) {
        if value == number {
            tied += 1;
        } else if if asc { value < number } else { value > number } {
            ahead += 1;
        }
    }
    Some((kind.result(ahead, tied)?, end))
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Extent {
    Highest,
    Lowest,
}

pub(super) fn nth(
    extent: Extent,
    tokens: &[Token],
    pos: usize,
    context: &mut Evaluation<'_>,
) -> Option<(f64, usize)> {
    let ([range, k], end) = arguments(tokens, pos)?;
    let range = exact_range(range?)?;
    let k = eval_complete(k?, context)?;
    let Some(table) = context.table else {
        return Some((0.0, end));
    };
    let (rows, cols) = range.loaded_shape(table);
    if k.fract() != 0.0 || k < 1.0 || k > rows.saturating_mul(cols) as f64 {
        return None;
    }
    let k = k as usize;
    let mut values: Vec<_> = numbers(range, table).collect();
    if k > values.len() {
        return None;
    }
    let index = match extent {
        Extent::Highest => values.len() - k,
        Extent::Lowest => k - 1,
    };
    let (_, value, _) = values.select_nth_unstable_by(index, f64::total_cmp);
    Some((*value, end))
}
