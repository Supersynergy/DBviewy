//! Rank rules shared by scalar evaluation and the fixed-range column fill.

#[derive(Clone, Copy, Debug)]
pub(super) enum Kind {
    Equal,
    Average,
}

impl Kind {
    pub(super) fn named(name: &str) -> Option<Self> {
        [
            ("RANK", Self::Equal),
            ("RANG", Self::Equal),
            ("RANK.EQ", Self::Equal),
            ("RANG.GLEICH", Self::Equal),
            ("RANK.AVG", Self::Average),
            ("RANG.MITTELW", Self::Average),
        ]
        .into_iter()
        .find(|(label, _)| label.eq_ignore_ascii_case(name))
        .map(|(_, kind)| kind)
    }

    pub(super) fn result(self, ahead: usize, tied: usize) -> Option<f64> {
        let last_tie_offset = tied.checked_sub(1)? as f64;
        let first = ahead as f64 + 1.0;
        Some(match self {
            Self::Equal => first,
            Self::Average => first + last_tie_offset / 2.0,
        })
    }
}

pub(super) fn ascending(order: f64) -> Option<bool> {
    order.is_finite().then_some(order != 0.0)
}
