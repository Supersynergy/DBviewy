//! Token boundaries shared by the evaluator and column formula fill.

pub(crate) fn has_intl_suffix(bytes: &[u8], start: usize) -> bool {
    bytes
        .get(start..)
        .and_then(|tail| tail.get(..5))
        .is_some_and(|suffix| suffix.eq_ignore_ascii_case(b".INTL"))
}

fn digits(bytes: &[u8], mut end: usize) -> usize {
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    end
}

/// A leading sign is an operator; the sign in an exponent is part of the
/// number. Return None for a bare decimal point or an incomplete exponent.
pub(crate) fn number_end(bytes: &[u8], start: usize) -> Option<usize> {
    let integer_end = digits(bytes, start);
    let mut end = integer_end;
    if bytes.get(end) == Some(&b'.') {
        end = digits(bytes, end + 1);
        if integer_end == start && end == start + 1 {
            return None;
        }
    } else if end == start {
        return None;
    }
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        let mut exponent = end + 1;
        if matches!(bytes.get(exponent), Some(b'+' | b'-')) {
            exponent += 1;
        }
        end = digits(bytes, exponent);
        if end == exponent {
            return None;
        }
    }
    Some(end)
}

/// ANZAHL2 followed by an argument list is a function, not a cell address.
/// Keep the distinction in both tokenisation and relative-row shifting.
pub(crate) fn numbered_function(bytes: &[u8], start: usize, end: usize) -> bool {
    bytes[start..end].eq_ignore_ascii_case(b"ANZAHL2")
        && bytes[end..].iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'(')
}
