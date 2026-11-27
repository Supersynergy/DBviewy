//! Shared paired statistics for the evaluator and the fixed-range forecast fill.
use super::{CellValue, DataTable};

pub(super) fn is_forecast(name: &str) -> bool {
    ["FORECAST.LINEAR", "PROGNOSE.LINEAR", "FORECAST", "PROGNOSE"]
        .iter()
        .any(|alias| alias.eq_ignore_ascii_case(name))
}

#[derive(Clone, Copy)]
pub(super) struct Block {
    pub(super) top: usize,
    pub(super) left: usize,
    rows: usize,
    cols: usize,
}

impl Block {
    pub(super) fn through(a: (usize, usize), b: (usize, usize)) -> Option<Self> {
        Some(Self {
            top: a.0.min(b.0),
            left: a.1.min(b.1),
            rows: a.0.abs_diff(b.0).checked_add(1)?,
            cols: a.1.abs_diff(b.1).checked_add(1)?,
        })
    }

    pub(super) fn common_loaded(self, other: Self, table: &DataTable) -> Option<(usize, usize)> {
        if (self.rows, self.cols) != (other.rows, other.cols) {
            return None;
        }
        // Read matching offsets only, and never iterate beyond the loaded
        // rectangle even when the spreadsheet references a huge range.
        Some((
            self.rows
                .min(table.row_count().saturating_sub(self.top.max(other.top))),
            self.cols
                .min(table.col_count().saturating_sub(self.left.max(other.left))),
        ))
    }
}

fn number(cell: &CellValue) -> Option<f64> {
    match cell {
        CellValue::Int(n) => Some(*n as f64),
        CellValue::Float(n) if n.is_finite() => Some(*n),
        _ => None,
    }
}

pub(super) fn fit(table: &DataTable, y: Block, x: Block) -> Option<PairedMoments> {
    let (rows, cols) = y.common_loaded(x, table)?;
    let mut moments = None;
    for row in 0..rows {
        for col in 0..cols {
            if let (Some(y), Some(x)) = (
                table.get(y.top + row, y.left + col).and_then(number),
                table.get(x.top + row, x.left + col).and_then(number),
            ) {
                match &mut moments {
                    Some(m) => PairedMoments::push(m, y, x),
                    None => moments = Some(PairedMoments::new(y, x)),
                }
            }
        }
    }
    moments
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

    fn scaled_position(&self, value: f64) -> f64 {
        if self.halved {
            (value * 0.5 - self.origin * 0.5) / self.scale
        } else {
            let delta = value - self.origin;
            if delta.is_finite() {
                delta / self.scale
            } else {
                ((value * 0.5 - self.origin * 0.5) / self.scale) * 2.0
            }
        }
    }

    fn absolute(&self, scaled: f64) -> f64 {
        if self.halved {
            scaled.mul_add(self.scale, self.origin * 0.5) * 2.0
        } else {
            scaled.mul_add(self.scale, self.origin)
        }
    }

    fn push(&mut self, value: f64, count: f64) -> f64 {
        let delta = value - self.mean;
        self.mean += delta / count;
        self.squared += delta * (value - self.mean);
        delta
    }
}

pub(super) struct PairedMoments {
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

    pub(super) fn estimate(&self, x: f64) -> Option<f64> {
        if !x.is_finite() || self.x.squared <= 0.0 {
            return None;
        }
        if self.cross == 0.0 {
            let value = self.y.absolute(self.y.mean);
            return value.is_finite().then_some(value);
        }
        // Evaluate in centred coordinates so the prediction near the
        // sample does not subtract two large, rounded absolute values.
        let dx = self.x.scaled_position(x) - self.x.mean;
        let y = (self.cross / self.x.squared).mul_add(dx, self.y.mean);
        let value = self.y.absolute(y);
        if value.is_finite() {
            return Some(value);
        }
        // A distant x on a very small scale can overflow in normalised
        // coordinates even when the unscaled slope and prediction fit.
        let slope = self.slope()?;
        let mean_x = self.x.absolute(self.x.mean);
        let mean_y = self.y.absolute(self.y.mean);
        let delta = x - mean_x;
        let value = if delta.is_finite() {
            slope.mul_add(delta, mean_y)
        } else {
            (slope * 2.0).mul_add(x * 0.5 - mean_x * 0.5, mean_y)
        };
        value.is_finite().then_some(value)
    }

    pub(super) fn correlation(&self) -> Option<f64> {
        if self.x.squared <= 0.0 || self.y.squared <= 0.0 {
            return None;
        }
        let r = (self.cross / self.x.squared.sqrt() / self.y.squared.sqrt()).clamp(-1.0, 1.0);
        r.is_finite().then_some(r)
    }

    pub(super) fn slope(&self) -> Option<f64> {
        if self.x.squared <= 0.0 {
            return None;
        }
        let value = {
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
        };
        value.is_finite().then_some(value)
    }

    pub(super) fn covariance(&self, sample: bool) -> Option<f64> {
        let divisor = self.count - usize::from(sample);
        if divisor == 0 {
            return None;
        }
        let relative = self.cross / divisor as f64;
        let half = (1 + usize::from(self.y.halved)) * (1 + usize::from(self.x.halved));
        let (larger, smaller) = (
            self.y.scale.max(self.x.scale),
            self.y.scale.min(self.x.scale),
        );
        let value = (relative * larger) * smaller * half as f64;
        value.is_finite().then_some(value)
    }
}
