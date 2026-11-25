//! Spreadsheet clipboard records without changing clipboard transport.
use std::borrow::Cow;

pub(super) fn write_field(out: &mut String, field: &str, single_column: bool) {
    // An explicit empty field preserves a fully empty last row in a
    // one-column block, distinct from the optional record terminator.
    if (single_column && field.is_empty()) || field.contains(['\t', '\r', '\n', '"']) {
        out.push('"');
        let mut rest = field;
        while let Some((before, after)) = rest.split_once('"') {
            out.push_str(before);
            out.push_str("\"\"");
            rest = after;
        }
        out.push_str(rest);
        out.push('"');
    } else {
        out.push_str(field);
    }
}

fn after_line_break(text: &str) -> &str {
    text.strip_prefix("\r\n").unwrap_or(&text[1..])
}

pub(super) fn plain_lines(mut text: &str) -> impl Iterator<Item = &str> {
    std::iter::from_fn(move || {
        if text.is_empty() {
            None
        } else if let Some(end) = text.find(['\r', '\n']) {
            let row = &text[..end];
            text = after_line_break(&text[end..]);
            Some(row)
        } else {
            Some(std::mem::take(&mut text))
        }
    })
}

fn plain_field<'a>(rest: &mut &'a str) -> &'a str {
    let end = rest.find(['\t', '\r', '\n']).unwrap_or(rest.len());
    let field = &rest[..end];
    *rest = &rest[end..];
    field
}

fn quoted_field<'a>(rest: &mut &'a str) -> Option<Cow<'a, str>> {
    let mut input = rest.strip_prefix('"')?;
    let mut unescaped = None;
    while let Some((before, after)) = input.split_once('"') {
        if let Some(input_after_escape) = after.strip_prefix('"') {
            let buffer = unescaped.get_or_insert_with(String::new);
            buffer.push_str(before);
            buffer.push('"');
            input = input_after_escape;
        } else {
            if !after.is_empty() && !after.starts_with(['\t', '\r', '\n']) {
                return None;
            }
            *rest = after;
            return Some(if let Some(mut buffer) = unescaped {
                buffer.push_str(before);
                Cow::Owned(buffer)
            } else {
                Cow::Borrowed(before)
            });
        }
    }
    None
}

pub(super) fn quoted_grid(text: &str) -> Option<Vec<Vec<Cow<'_, str>>>> {
    // Do not reinterpret a standalone quoted identifier, and do not
    // allocate decoded fields for the common plain TSV grid.
    if !text.contains('"') || !text.contains(['\r', '\n', '\t']) {
        return None;
    }
    let mut rest = text;
    let mut rows = Vec::new();
    while !rest.is_empty() {
        let mut row = Vec::new();
        loop {
            row.push(if rest.starts_with('"') {
                quoted_field(&mut rest)?
            } else {
                Cow::Borrowed(plain_field(&mut rest))
            });
            if let Some(next) = rest.strip_prefix('\t') {
                rest = next;
            } else {
                if !rest.is_empty() {
                    rest = after_line_break(rest);
                }
                break;
            }
        }
        rows.push(row);
    }
    Some(rows)
}
