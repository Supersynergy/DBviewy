//! Encode cell-only time edits without changing the worksheet's number format.
use anyhow::{Result, bail, ensure};
use calamine::Data;
use octa::data::{CellValue, times::NANOS_PER_DAY};

#[derive(Clone, Copy)]
pub(super) enum TimeCell {
    Clock,
    Duration,
}

pub(super) fn is_time(value: &CellValue) -> bool {
    matches!(value, CellValue::Time(_) | CellValue::Duration(_))
}

pub(super) fn same_family(original: &CellValue, value: &CellValue) -> bool {
    matches!(
        (original, value),
        (CellValue::Time(_), CellValue::Time(_)) | (CellValue::Duration(_), CellValue::Duration(_))
    )
}

impl TimeCell {
    pub(super) fn from_raw(cell: &Data) -> Option<Self> {
        match cell {
            Data::DateTime(value) if value.is_duration() => Some(Self::Duration),
            Data::DateTime(value) if (0.0..1.0).contains(&value.as_f64()) => Some(Self::Clock),
            _ => None,
        }
    }

    pub(super) fn encode(
        self,
        value: &CellValue,
        address: &str,
        date_1904: bool,
    ) -> Result<(&'static str, String)> {
        let ns = match value {
            CellValue::Time(ns) | CellValue::Duration(ns) => *ns,
            _ => bail!("Use a time or duration at {address}"),
        };
        ensure!(
            ns.unsigned_abs().is_multiple_of(1_000_000),
            "Use at most millisecond precision in {address}"
        );
        match self {
            // In a mixed column, the reader promotes clock times to
            // Duration. The source cell's formatting still restricts it
            // to one day, even when the grid editor accepts longer inputs.
            Self::Clock => ensure!(
                (0..NANOS_PER_DAY).contains(&ns),
                "The clock format at {address} requires a time from 00:00 to before 24:00"
            ),
            Self::Duration => ensure!(
                ns >= 0 || date_1904,
                "Negative durations at {address} require an existing 1904 workbook or a separate export"
            ),
        }
        let serial = ns as f64 / NANOS_PER_DAY as f64;
        Ok(("n", format!("<v>{serial}</v>")))
    }
}
