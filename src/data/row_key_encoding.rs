//! Shared field boundaries for exact row identity and the comparison view.

use std::borrow::Cow;

use super::CellValue;

/// The existing display comparison, borrowing raw text where possible.
/// Missing and null cells have the same empty display as before.
pub(crate) fn displayed(cell: Option<&CellValue>) -> Cow<'_, str> {
    match cell {
        None | Some(CellValue::Null) => "".into(),
        Some(
            CellValue::String(s)
            | CellValue::Date(s)
            | CellValue::DateTime(s)
            | CellValue::Nested(s),
        ) => s.as_str().into(),
        Some(v) => v.to_string().into(),
    }
}

/// A self-delimiting field: base-128 byte length, then the original UTF-8.
/// The receiver can retain the chunks as a key or stream them to a hash.
pub(crate) fn append_field(value: &str, mut append: impl FnMut(&[u8])) {
    let mut prefix = [0u8; usize::BITS.div_ceil(7) as usize];
    let mut len = value.len();
    let mut n = 0;
    while len >= 128 {
        prefix[n] = (len as u8 & 0x7f) | 0x80;
        len >>= 7;
        n += 1;
    }
    prefix[n] = len as u8;
    append(&prefix[..=n]);
    append(value.as_bytes());
}
