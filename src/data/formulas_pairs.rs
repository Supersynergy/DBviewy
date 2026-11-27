//! Paired moments, with bounded space and no independent compaction of inputs.

use super::control::{arguments, unparen};
use super::{CellValue, DataTable, Evaluation, FormulaToken as Token, eval_complete};

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
    };
    Some((value, end))
}
