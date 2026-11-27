//! Token boundaries shared by the evaluator and column formula fill.

pub(crate) fn column_index(letters: &[u8]) -> Option<usize> {
    letters
        .iter()
        .try_fold(0usize, |col, &letter| {
            if !letter.is_ascii_alphabetic() {
                return None;
            }
            col.checked_mul(26)?
                .checked_add(usize::from(letter.to_ascii_uppercase() - b'A' + 1))
        })?
        .checked_sub(1)
}

/// A dotted function suffix, such as .INTL or .EXC. The evaluator still
/// requires the complete name to be registered.
pub(crate) fn dotted_suffix_end(bytes: &[u8], start: usize) -> Option<usize> {
    let suffix = bytes.get(start..)?.strip_prefix(b".")?;
    let letters = suffix
        .iter()
        .take_while(|b| b.is_ascii_alphabetic())
        .count();
    (letters != 0).then_some(start + 1 + letters)
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

/// Known names ending in digits need an argument list to be functions.
/// Keep the distinction in both tokenisation and relative-row shifting.
pub(crate) fn numbered_function(bytes: &[u8], start: usize, end: usize) -> bool {
    let name = &bytes[start..end];
    [b"ANZAHL2".as_slice(), b"SUMXMY2", b"SUMMEXMY2"]
        .iter()
        .any(|alias| name.eq_ignore_ascii_case(alias))
        && bytes[end..].iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'(')
}
