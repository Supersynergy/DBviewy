//! Paired moments, with bounded space and no independent compaction of inputs.

use super::control::{arguments, unparen};
use super::criteria::{Rectangle, rectangle};
use super::{CellValue, Evaluation, FormulaToken as Token};

#[derive(Clone, Copy, Debug)]
pub(super) enum Kind {
    Correlation,
    SampleCovariance,
    PopulationCovariance,
    Slope,
}

impl Kind {
    pub(super) fn named(name: &str) -> Option<Self> {
        use Kind::*;
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
        ]
        .into_iter()
        .find(|(alias, _)| alias.eq_ignore_ascii_case(name))
        .map(|(_, kind)| kind)
    }
}

fn exact_range(tokens: &[Token]) -> Option<Rectangle> {
    let tokens = unparen(tokens);
    let (range, end) = rectangle(tokens, 0)?;
    (end == tokens.len()).then_some(range)
}

fn number(cell: &CellValue) -> Option<f64> {
    match cell {
        CellValue::Int(n) => Some(*n as f64),
        CellValue::Float(n) if n.is_finite() => Some(*n),
        _ => None,
    }
}

pub(super) fn evaluate(
    kind: Kind,
    tokens: &[Token],
    pos: usize,
    context: &mut Evaluation<'_>,
) -> Option<(f64, usize)> {
    let ([left, right], end) = arguments(tokens, pos)?;
    let left = exact_range(left?)?;
    let right = exact_range(right?)?;
    let Some(table) = context.table else {
        // In an inactive branch, only syntax is checked.
        return Some((0.0, end));
    };
    if left.shape() != right.shape() {
        return None;
    }
    let (left_rows, left_cols) = left.loaded_shape(table);
    let (right_rows, right_cols) = right.loaded_shape(table);
    let mut moments = None;
    for row in 0..left_rows.min(right_rows) {
        for col in 0..left_cols.min(right_cols) {
            let (lr, lc) = left.at(row, col);
            let (rr, rc) = right.at(row, col);
            if let (Some(y), Some(x)) = (
                table.get(lr, lc).and_then(number),
                table.get(rr, rc).and_then(number),
            ) {
                match &mut moments {
                    Some(m) => PairedMoments::push(m, y, x),
                    None => moments = Some(PairedMoments::new(y, x)),
                }
            }
        }
    }
    Some((moments?.result(kind)?, end))
}

struct Axis {
    origin: f64,
    scale: f64,
    halved: bool,
    mean: f64,
    squared: f64,
}

impl Axis {
    fn new(origin: f64) -> Self {
        Self {
            origin,
            scale: 0.0,
            halved: false,
            mean: 0.0,
            squared: 0.0,
        }
    }

    // Work in deviations from the first pair, not raw large values. When
    // the scale grows, express existing moments in the new coordinates.
    fn normalise(&mut self, value: f64) -> (f64, f64) {
        let mut offset = value - self.origin;
        if offset.is_infinite() || self.halved {
            if !self.halved {
                self.halved = true;
                self.scale *= 0.5;
            }
            offset = value * 0.5 - self.origin * 0.5;
        }
        let mut shrink = 1.0;
        if offset.abs() > self.scale {
            shrink = self.scale / offset.abs();
            self.mean *= shrink;
            self.squared *= shrink * shrink;
            self.scale = offset.abs();
        }
        let scaled = if self.scale == 0.0 {
            0.0
        } else {
            offset / self.scale
        };
        (scaled, shrink)
    }

    fn push(&mut self, value: f64, count: f64) -> f64 {
        let delta = value - self.mean;
        self.mean += delta / count;
        self.squared += delta * (value - self.mean);
        delta
    }
}

struct PairedMoments {
    count: usize,
    y: Axis,
    x: Axis,
    cross: f64,
}

impl PairedMoments {
    fn new(y: f64, x: f64) -> Self {
        Self {
            count: 1,
            y: Axis::new(y),
            x: Axis::new(x),
            cross: 0.0,
        }
    }

    fn push(&mut self, y: f64, x: f64) {
        self.count += 1;
        let (y, shrink_y) = self.y.normalise(y);
        let (x, shrink_x) = self.x.normalise(x);
        let count = self.count as f64;
        self.y.push(y, count);
        let dx = self.x.push(x, count);
        self.cross *= shrink_y * shrink_x;
        self.cross += dx * (y - self.y.mean);
    }

    fn result(&self, kind: Kind) -> Option<f64> {
        use Kind::*;
        let value = match kind {
            Correlation if self.x.squared > 0.0 && self.y.squared > 0.0 => {
                (self.cross / self.x.squared.sqrt() / self.y.squared.sqrt()).clamp(-1.0, 1.0)
            }
            Slope if self.x.squared > 0.0 => {
                let relative = self.cross / self.x.squared;
                let relative = match (self.y.halved, self.x.halved) {
                    (true, false) => relative * 2.0,
                    (false, true) => relative * 0.5,
                    _ => relative,
                };
                if relative == 0.0 {
                    return Some(0.0);
                }
                let ratio = self.y.scale / self.x.scale;
                if ratio.is_finite() && ratio != 0.0 {
                    relative * ratio
                } else {
                    let partial = relative * self.y.scale;
                    if partial.is_finite() && partial != 0.0 {
                        partial / self.x.scale
                    } else {
                        (relative / self.x.scale) * self.y.scale
                    }
                }
            }
            SampleCovariance | PopulationCovariance => {
                let divisor = self.count - usize::from(matches!(kind, SampleCovariance));
                if divisor == 0 {
                    return None;
                }
                let relative = self.cross / divisor as f64;
                let half = (1 + usize::from(self.y.halved)) * (1 + usize::from(self.x.halved));
                let (larger, smaller) = (
                    self.y.scale.max(self.x.scale),
                    self.y.scale.min(self.x.scale),
                );
                (relative * larger) * smaller * half as f64
            }
            _ => return None,
        };
        value.is_finite().then_some(value)
    }
}
