//! Online sample and population dispersion. Shared with Insert Column
//! so an expanding range does not need to scan its earlier rows again.

use super::CellValue;

#[derive(Clone, Copy, Debug)]
pub(super) enum Kind {
    SampleDeviation,
    PopulationDeviation,
    SampleVariance,
    PopulationVariance,
}

impl Kind {
    pub(super) fn named(name: &str) -> Option<Self> {
        use Kind::*;
        [
            ("STDEV.S", SampleDeviation),
            ("STABW.S", SampleDeviation),
            ("STDEV", SampleDeviation),
            ("STABW", SampleDeviation),
            ("STDEV.P", PopulationDeviation),
            ("STABW.N", PopulationDeviation),
            ("STDEVP", PopulationDeviation),
            ("STABWN", PopulationDeviation),
            ("VAR.S", SampleVariance),
            ("VAR", SampleVariance),
            ("VARIANZ", SampleVariance),
            ("VAR.P", PopulationVariance),
            ("VARP", PopulationVariance),
            ("VARIANZEN", PopulationVariance),
        ]
        .into_iter()
        .find(|(word, _)| word.eq_ignore_ascii_case(name))
        .map(|(_, found)| found)
    }
}

/// A reference may be a number or numeric text from an imported column.
/// The accumulator ignores non-finite inputs; some other aggregates do not.
pub(super) fn cell_number(cell: &CellValue) -> Option<f64> {
    match cell {
        CellValue::Int(n) => Some(*n as f64),
        CellValue::Float(n) => Some(*n),
        CellValue::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// Welford's update in shifted, scaled coordinates. Subtracting the first
/// value retains small differences between large nearby observations.
/// The scale keeps the sum of squared deviations representable.
#[derive(Default)]
pub(super) struct Moments {
    count: usize,
    origin: f64,
    scale: f64,
    mean: f64,
    squared: f64,
    halved: bool,
}

impl Moments {
    pub(super) fn add(&mut self, value: f64) {
        if !value.is_finite() {
            return;
        }
        if self.count == 0 {
            self.origin = value;
            self.count = 1;
            return;
        }
        let mut shifted = value - self.origin;
        if self.halved || shifted.is_infinite() {
            // Opposite signs can overflow even during centring. Halve
            // before subtracting; the normalised old sample is unchanged.
            if !self.halved {
                self.scale *= 0.5;
                self.halved = true;
            }
            shifted = value * 0.5 - self.origin * 0.5;
        }
        if shifted.abs() > self.scale {
            let previous = self.scale / shifted.abs();
            self.mean *= previous;
            self.squared *= previous * previous;
            self.scale = shifted.abs();
        }
        let x = if self.scale == 0.0 {
            0.0
        } else {
            shifted / self.scale
        };
        self.count += 1;
        let delta = x - self.mean;
        self.mean += delta / self.count as f64;
        self.squared = delta.mul_add(x - self.mean, self.squared);
    }

    pub(super) fn result(&self, kind: Kind) -> Option<f64> {
        let n = match kind {
            Kind::SampleDeviation | Kind::SampleVariance => self.count.checked_sub(1)?,
            Kind::PopulationDeviation | Kind::PopulationVariance => self.count,
        };
        if n == 0 {
            return None;
        }
        // Take the square root before restoring the units so a finite
        // standard deviation does not require a representable variance.
        let deviation = self.scale * (self.squared.max(0.0) / n as f64).sqrt();
        let deviation = if self.halved {
            deviation * 2.0
        } else {
            deviation
        };
        let result = match kind {
            Kind::SampleVariance | Kind::PopulationVariance => deviation * deviation,
            _ => deviation,
        };
        result.is_finite().then_some(result)
    }
}
