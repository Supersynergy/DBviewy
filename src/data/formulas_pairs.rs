//! Paired moments, with bounded space and no independent compaction of inputs.

use super::control::{arguments, unparen};
use super::squares::Squares;
use super::{CellValue, DataTable, Evaluation, FormulaToken as Token, eval_complete};
use crate::data::decimal::{decimal_parts, parse_decimal};

#[path = "formula_pair_math.rs"]
mod math;
use math::Block;

#[derive(Clone, Copy, Debug)]
pub(super) enum Kind {
    Correlation,
    SampleCovariance,
    PopulationCovariance,
    Slope,
    Intercept,
    Determination,
    SquaredDifference,
    Forecast,
}

impl Kind {
    pub(super) fn named(name: &str) -> Option<Self> {
        use Kind::*;
        if math::is_forecast(name) {
            return Some(Forecast);
        }
        [
            ("CORREL", Correlation),
            ("KORREL", Correlation),
            ("PEARSON", Correlation),
            ("COVARIANCE.S", SampleCovariance),
            ("KOVARIANZ.S", SampleCovariance),
            ("COVARIANCE.P", PopulationCovariance),
            ("KOVARIANZ.P", PopulationCovariance),
            ("COVAR", PopulationCovariance),
            ("KOVAR", PopulationCovariance),
            ("SLOPE", Slope),
            ("STEIGUNG", Slope),
            ("INTERCEPT", Intercept),
            ("ACHSENABSCHNITT", Intercept),
            ("RSQ", Determination),
            ("BESTIMMTHEITSMASS", Determination),
            ("SUMXMY2", SquaredDifference),
            ("SUMMEXMY2", SquaredDifference),
        ]
        .into_iter()
        .find(|(alias, _)| alias.eq_ignore_ascii_case(name))
        .map(|(_, kind)| kind)
    }
}

fn exact_range(tokens: &[Token]) -> Option<Block> {
    match unparen(tokens) {
        [Token::CellRef(r, c)] => Block::through((*r, *c), (*r, *c)),
        [
            Token::CellRef(r, c),
            Token::Range,
            Token::CellRef(end_r, end_c),
        ] => Block::through((*r, *c), (*end_r, *end_c)),
        _ => None,
    }
}

pub(super) fn evaluate(
    kind: Kind,
    tokens: &[Token],
    pos: usize,
    context: &mut Evaluation<'_>,
) -> Option<(f64, usize)> {
    let (prediction_at, left, right, end) = if matches!(kind, Kind::Forecast) {
        let ([x, y_range, x_range], end) = arguments(tokens, pos)?;
        let x = eval_complete(x?, context)?;
        (Some(x), y_range, x_range, end)
    } else {
        let ([left, right], end) = arguments(tokens, pos)?;
        (None, left, right, end)
    };
    let left = exact_range(left?)?;
    let right = exact_range(right?)?;
    let Some(table) = context.table else {
        // In an inactive branch, only syntax is checked.
        return Some((0.0, end));
    };
    if matches!(kind, Kind::SquaredDifference) {
        return Some((squared_differences(table, left, right)?, end));
    }
    let moments = math::fit(table, left, right)?;
    let value = match kind {
        Kind::Correlation => moments.correlation()?,
        Kind::SampleCovariance => moments.covariance(true)?,
        Kind::PopulationCovariance => moments.covariance(false)?,
        Kind::Slope => moments.slope()?,
        Kind::Intercept => moments.estimate(0.0)?,
        Kind::Determination => {
            let r = moments.correlation()?;
            r * r
        }
        Kind::Forecast => moments.estimate(prediction_at?)?,
        Kind::SquaredDifference => unreachable!("paired errors do not need fitted moments"),
    };
    Some((value, end))
}

fn squared_differences(table: &DataTable, left: Block, right: Block) -> Option<f64> {
    let (rows, cols) = left.common_loaded(right, table)?;
    let mut sum = Squares::default();
    for col in 0..cols {
        let a_col = left.left + col;
        let b_col = right.left + col;
        let a_decimal = decimal_parts(&table.columns[a_col].data_type);
        let b_decimal = decimal_parts(&table.columns[b_col].data_type);
        for row in 0..rows {
            let a = table
                .get(left.top + row, a_col)
                .and_then(|v| Measurement::read(v, a_decimal));
            let b = table
                .get(right.top + row, b_col)
                .and_then(|v| Measurement::read(v, b_decimal));
            if let (Some(a), Some(b)) = (a, b) {
                sum.add(a.difference(b))?;
            }
        }
    }
    sum.value()
}

#[derive(Clone, Copy)]
enum Measurement {
    Exact(i128, i8),
    Float(f64),
}

impl Measurement {
    fn read(value: &CellValue, decimal: Option<(u8, i8)>) -> Option<Self> {
        match value {
            CellValue::Int(n) => Some(Self::Exact(i128::from(*n), 0)),
            CellValue::Float(n) if n.is_finite() => Some(Self::Float(*n)),
            CellValue::String(text) => {
                let (precision, scale) = decimal?;
                Some(Self::Exact(parse_decimal(text, precision, scale)?, scale))
            }
            _ => None,
        }
    }

    fn difference(self, other: Self) -> f64 {
        if let (Self::Exact(a, sa), Self::Exact(b, sb)) = (self, other) {
            let scale = sa.max(sb);
            let shift =
                |n: i128, from: i8| n.checked_mul(10_i128.pow(u32::from((scale - from) as u8)));
            // Subtract before rounding to a float so e.g. two 64-bit
            // integer IDs or nearby decimal measurements do not round equal.
            let difference = if sa == sb {
                a.checked_sub(b)
            } else {
                shift(a, sa)
                    .zip(shift(b, sb))
                    .and_then(|(a, b)| a.checked_sub(b))
            };
            if let Some(delta) = difference {
                return if scale == 0 {
                    delta as f64
                } else {
                    delta as f64 / 10_f64.powi(i32::from(scale))
                };
            }
        }
        self.as_f64() - other.as_f64()
    }

    fn as_f64(self) -> f64 {
        match self {
            Self::Exact(raw, 0) => raw as f64,
            Self::Exact(raw, scale) => raw as f64 / 10_f64.powi(i32::from(scale)),
            Self::Float(n) => n,
        }
    }
}
