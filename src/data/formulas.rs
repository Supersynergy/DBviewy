//! Excel-like formula evaluation against a [`DataTable`].
//!
//! Supports cell references (e.g. `A1`, `BC42`), numeric literals, the four
//! arithmetic operators, parenthesised sub-expressions, scalar functions and
//! numeric aggregates over ranges.
//! Re-exported from [`crate::data`] so call sites keep their existing import
//! path (`use octa::data::evaluate_formula`).

use super::{CellValue, DataTable};

/// Snapshot of one referenced cell that could not be coerced to a number
/// during formula evaluation: zero-indexed `(row, col)` plus a short display
/// of the offending content. Used to surface a parse-error banner when the
/// Insert Column dialog runs a formula against a column that turns out to be
/// non-numeric in some rows.
#[derive(Debug, Clone)]
pub struct FormulaBadCell {
    pub row: usize,
    pub col: usize,
    pub content: String,
}

/// Result of evaluating a formula for one row.
///
/// `value` is `Some(_)` when every referenced cell coerced cleanly; `None`
/// when at least one didn't (or when the formula was malformed). `bad_cell`
/// carries the first offender so callers can build a "first non-numeric
/// cell: X" banner without re-walking the row.
#[derive(Debug, Clone)]
pub struct FormulaOutcome {
    pub value: Option<f64>,
    pub bad_cell: Option<FormulaBadCell>,
}

/// Evaluate a simple Excel-like formula.
/// Supports cell references (e.g. A1, B2), numeric literals, and operators +, -, *, /.
/// Column letters: A=0, B=1, ..., Z=25, AA=26, etc. Row numbers are 1-based.
/// Returns the computed f64 result, or None if the formula is invalid.
pub fn evaluate_formula(formula: &str, table: &DataTable) -> Option<f64> {
    evaluate_formula_with_diagnostics(formula, table).value
}

/// Like [`evaluate_formula`] but also reports the first cell reference that
/// pointed at a non-numeric value (e.g. a string column or an unparseable
/// piece of text). Returns `value: None` when any referenced cell failed.
pub fn evaluate_formula_with_diagnostics(formula: &str, table: &DataTable) -> FormulaOutcome {
    let expr = formula.trim();
    if expr.is_empty() {
        return FormulaOutcome {
            value: None,
            bad_cell: None,
        };
    }
    let Some(tokens) = tokenize_formula(expr) else {
        return FormulaOutcome {
            value: None,
            bad_cell: None,
        };
    };
    let mut bad: Option<FormulaBadCell> = None;
    let value = eval_expression(&tokens, 0, table, &mut bad)
        .filter(|(_, end)| *end == tokens.len())
        .map(|(value, _)| value);
    FormulaOutcome {
        value,
        bad_cell: bad,
    }
}

/// Resolve a cell reference like "A1", "BC42" to (row, col) zero-indexed.
fn parse_cell_ref(s: &str) -> Option<(usize, usize)> {
    let s = s.trim();
    let col_end = s.bytes().position(|b| b.is_ascii_digit())?;
    if col_end == 0 {
        return None;
    }
    let col_str = &s[..col_end];
    let row_str = &s[col_end..];
    if row_str.is_empty() {
        return None;
    }
    // Column: A=0, B=1, ..., Z=25, AA=26
    let mut col: usize = 0;
    for ch in col_str.chars() {
        if !ch.is_ascii_alphabetic() {
            return None;
        }
        col = col
            .checked_mul(26)?
            .checked_add(ch.to_ascii_uppercase() as usize - b'A' as usize + 1)?;
    }
    col -= 1; // zero-indexed
    let row: usize = row_str.parse::<usize>().ok()?;
    if row == 0 {
        return None;
    }
    Some((row - 1, col))
}

/// Try to read a cell as f64. Returns:
///
/// * `Ok(Some(f))` - numeric (int / float, or a Null treated as 0).
/// * `Ok(None)` - the cell is out of bounds (`table.get` returned None);
///   not a "bad value", just a missing one.
/// * `Err(FormulaBadCell)` - the cell exists but isn't a number. Includes a
///   short content snippet so the caller can build a user-facing error.
///
/// Strings are accepted only when they parse cleanly as f64; "abc" returns
/// `Err` rather than silently coercing to 0.0 (which previously corrupted
/// formula results when a referenced column was Utf8).
fn cell_as_f64(table: &DataTable, row: usize, col: usize) -> Result<Option<f64>, FormulaBadCell> {
    let Some(cell) = table.get(row, col) else {
        return Ok(None);
    };
    match cell {
        CellValue::Int(n) => Ok(Some(*n as f64)),
        CellValue::Float(f) => Ok(Some(*f)),
        CellValue::String(s) => match s.trim().parse::<f64>() {
            Ok(v) => Ok(Some(v)),
            Err(_) => Err(FormulaBadCell {
                row,
                col,
                content: snippet(s),
            }),
        },
        CellValue::Null => Ok(Some(0.0)),
        other => Err(FormulaBadCell {
            row,
            col,
            content: snippet(&other.to_string()),
        }),
    }
}

/// Trim long cell content to a short display snippet for error messages.
fn snippet(s: &str) -> String {
    const MAX: usize = 40;
    if s.chars().count() <= MAX {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(MAX).collect();
        t.push_str("...");
        t
    }
}

#[derive(Debug, Clone)]
enum FormulaToken {
    Number(f64),
    CellRef(usize, usize), // (row, col)
    Op(char),              // +, -, *, /
    Function(Function),
    Range,
    Separator,
    LParen,
    RParen,
}

#[derive(Clone, Copy, Debug)]
enum Function {
    Aggregate(Aggregate),
    Abs,
    Round,
}

fn tokenize_formula(expr: &str) -> Option<Vec<FormulaToken>> {
    // Evaluation uses recursive descent for nested parentheses and calls.
    const MAX_NESTING: usize = 64;
    let mut tokens = Vec::new();
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0;
    let mut depth = 0usize;
    while i < chars.len() {
        match chars[i] {
            c if c.is_ascii_whitespace() => {
                i += 1;
            }
            '+' | '-' | '*' | '/' => {
                tokens.push(FormulaToken::Op(chars[i]));
                i += 1;
            }
            ':' => {
                tokens.push(FormulaToken::Range);
                i += 1;
            }
            ',' | ';' => {
                tokens.push(FormulaToken::Separator);
                i += 1;
            }
            '(' => {
                depth += 1;
                if depth > MAX_NESTING {
                    return None;
                }
                tokens.push(FormulaToken::LParen);
                i += 1;
            }
            ')' => {
                depth = depth.checked_sub(1)?;
                tokens.push(FormulaToken::RParen);
                i += 1;
            }
            c if c.is_ascii_digit() || c == '.' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                let num_str: String = chars[start..i].iter().collect();
                let val: f64 = num_str.parse().ok()?;
                tokens.push(FormulaToken::Number(val));
            }
            c if c.is_ascii_alphabetic() => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_alphanumeric()) {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                let function = match word.to_ascii_uppercase().as_str() {
                    "SUM" | "SUMME" => Function::Aggregate(Aggregate::Sum),
                    "AVERAGE" | "MITTELWERT" => Function::Aggregate(Aggregate::Average),
                    "MIN" => Function::Aggregate(Aggregate::Min),
                    "MAX" => Function::Aggregate(Aggregate::Max),
                    "COUNT" | "ANZAHL" => Function::Aggregate(Aggregate::Count),
                    "MEDIAN" => Function::Aggregate(Aggregate::Median),
                    "ABS" => Function::Abs,
                    "ROUND" | "RUNDEN" => Function::Round,
                    _ => {
                        let (row, col) = parse_cell_ref(&word)?;
                        tokens.push(FormulaToken::CellRef(row, col));
                        continue;
                    }
                };
                tokens.push(FormulaToken::Function(function));
            }
            _ => return None,
        }
    }
    (depth == 0).then_some(tokens)
}

/// Recursive descent parser: expression = term (('+' | '-') term)*
///
/// `bad` captures the first non-numeric cell ref encountered during the
/// walk. Once set, evaluation still continues so we can drain the full
/// token stream, but the final value is dropped by the caller (None).
fn eval_expression(
    tokens: &[FormulaToken],
    pos: usize,
    table: &DataTable,
    bad: &mut Option<FormulaBadCell>,
) -> Option<(f64, usize)> {
    let (mut left, mut p) = eval_term(tokens, pos, table, bad)?;
    while p < tokens.len() {
        match &tokens[p] {
            FormulaToken::Op('+') => {
                let (right, np) = eval_term(tokens, p + 1, table, bad)?;
                left += right;
                p = np;
            }
            FormulaToken::Op('-') => {
                let (right, np) = eval_term(tokens, p + 1, table, bad)?;
                left -= right;
                p = np;
            }
            _ => break,
        }
    }
    Some((left, p))
}

/// term = factor (('*' | '/') factor)*
fn eval_term(
    tokens: &[FormulaToken],
    pos: usize,
    table: &DataTable,
    bad: &mut Option<FormulaBadCell>,
) -> Option<(f64, usize)> {
    let (mut left, mut p) = eval_factor(tokens, pos, table, bad)?;
    while p < tokens.len() {
        match &tokens[p] {
            FormulaToken::Op('*') => {
                let (right, np) = eval_factor(tokens, p + 1, table, bad)?;
                left *= right;
                p = np;
            }
            FormulaToken::Op('/') => {
                let (right, np) = eval_factor(tokens, p + 1, table, bad)?;
                if right == 0.0 {
                    return None; // division by zero
                }
                left /= right;
                p = np;
            }
            _ => break,
        }
    }
    Some((left, p))
}

/// factor = ('+' | '-')* (Number | CellRef | function | '(' expression ')')
fn eval_factor(
    tokens: &[FormulaToken],
    mut pos: usize,
    table: &DataTable,
    bad: &mut Option<FormulaBadCell>,
) -> Option<(f64, usize)> {
    let mut negate = false;
    loop {
        match tokens.get(pos)? {
            FormulaToken::Op('-') => negate = !negate,
            FormulaToken::Op('+') => {}
            _ => break,
        }
        pos += 1;
    }
    let (value, end) = match tokens.get(pos)? {
        FormulaToken::Number(n) => Some((*n, pos + 1)),
        FormulaToken::Function(function) => eval_function(*function, tokens, pos + 1, table, bad),
        FormulaToken::CellRef(row, col) => {
            match cell_as_f64(table, *row, *col) {
                Ok(Some(v)) => Some((v, pos + 1)),
                // Out-of-bounds reference: treat as 0 so a partly-overrun
                // formula still produces a result for the rows that do fit.
                Ok(None) => Some((0.0, pos + 1)),
                Err(b) => {
                    if bad.is_none() {
                        *bad = Some(b);
                    }
                    // Tell the caller "this row can't be numerically
                    // resolved" by aborting the walk.
                    None
                }
            }
        }
        FormulaToken::LParen => {
            let (val, p) = eval_expression(tokens, pos + 1, table, bad)?;
            if p < tokens.len() && matches!(tokens[p], FormulaToken::RParen) {
                Some((val, p + 1))
            } else {
                None
            }
        }
        _ => None,
    }?;
    Some((if negate { -value } else { value }, end))
}

fn eval_function(
    function: Function,
    tokens: &[FormulaToken],
    pos: usize,
    table: &DataTable,
    bad: &mut Option<FormulaBadCell>,
) -> Option<(f64, usize)> {
    if let Function::Aggregate(aggregate) = function {
        return eval_aggregate(aggregate, tokens, pos, table, bad);
    }
    if !matches!(tokens.get(pos), Some(FormulaToken::LParen)) {
        return None;
    }
    let (value, end) = eval_expression(tokens, pos + 1, table, bad)?;
    let (value, end) = match function {
        Function::Abs => (value.abs(), end),
        Function::Round => {
            if !matches!(tokens.get(end), Some(FormulaToken::Separator)) {
                return None;
            }
            let (places, end) = eval_expression(tokens, end + 1, table, bad)?;
            (round_decimal(value, places)?, end)
        }
        Function::Aggregate(_) => unreachable!("aggregates are handled above"),
    };
    if !matches!(tokens.get(end), Some(FormulaToken::RParen)) {
        return None;
    }
    value.is_finite().then_some((value, end + 1))
}

/// Round the shortest decimal representation rather than scaling the binary
/// float. Scaling can put a decimal tie like 1.005 just below 100.5 and can
/// overflow even when the rounded result itself is representable.
fn round_decimal(value: f64, places: f64) -> Option<f64> {
    if !value.is_finite() || places.fract() != 0.0 {
        return None;
    }
    if value == 0.0 || places <= -310.0 {
        return Some(0.0);
    }
    if places >= 324.0 {
        return Some(value);
    }
    let places = places as i32;
    let decimal = if value.is_subnormal() {
        // The shortest spelling of the smallest float is 5e-324, but
        // its value is below the midpoint when rounding to 323 places.
        format!("{:.16e}", value.abs())
    } else {
        format!("{:e}", value.abs())
    };
    let (mantissa, exponent) = decimal.split_once('e')?;
    let keep = exponent.parse::<i32>().ok()? + places + 1;
    if keep < 0 {
        return Some(0.0);
    }
    let keep = keep as usize;
    let mut digits = mantissa.bytes().filter(|&digit| digit != b'.');
    let mut units = 0u64;
    for _ in 0..keep {
        let Some(digit) = digits.next() else {
            return Some(value);
        };
        units = units * 10 + u64::from(digit - b'0');
    }
    let Some(first_discarded) = digits.next() else {
        return Some(value);
    };
    if first_discarded >= b'5' {
        units += 1;
    }
    if units == 0 {
        return Some(0.0);
    }
    let rounded = format!("{units}e{}", -places).parse::<f64>().ok()?;
    rounded.is_finite().then_some(rounded.copysign(value))
}

#[derive(Clone, Copy, Debug)]
enum Aggregate {
    Sum,
    Average,
    Min,
    Max,
    Count,
    Median,
}

/// Aggregate references skip empty and non-numeric cells. Numeric text is useful
/// in imported tables and follows the same conversion as arithmetic references.
fn aggregate_cell(table: &DataTable, row: usize, col: usize) -> Option<f64> {
    match table.get(row, col)? {
        CellValue::Int(n) => Some(*n as f64),
        CellValue::Float(n) => Some(*n),
        CellValue::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn eval_aggregate(
    function: Aggregate,
    tokens: &[FormulaToken],
    mut pos: usize,
    table: &DataTable,
    bad: &mut Option<FormulaBadCell>,
) -> Option<(f64, usize)> {
    if !matches!(tokens.get(pos), Some(FormulaToken::LParen)) {
        return None;
    }
    pos += 1;
    let mut value: f64 = 0.0;
    let mut count = 0usize;
    let mut median_values = Vec::new();
    let mut add = |n: f64| {
        value = match function {
            Aggregate::Min if count > 0 => value.min(n),
            Aggregate::Max if count > 0 => value.max(n),
            Aggregate::Min | Aggregate::Max => n,
            Aggregate::Sum | Aggregate::Average => value + n,
            Aggregate::Count => value,
            Aggregate::Median => {
                if n.is_finite() {
                    median_values.push(n);
                }
                value
            }
        };
        count += 1;
    };
    loop {
        match tokens.get(pos..)? {
            [
                FormulaToken::CellRef(r1, c1),
                FormulaToken::Range,
                FormulaToken::CellRef(r2, c2),
                ..,
            ] => {
                // Intersect with the loaded table before walking the rectangle.
                let end_row = r1.max(r2).saturating_add(1).min(table.row_count());
                let end_col = c1.max(c2).saturating_add(1).min(table.col_count());
                for row in *r1.min(r2)..end_row {
                    for col in *c1.min(c2)..end_col {
                        if let Some(n) = aggregate_cell(table, row, col) {
                            add(n);
                        }
                    }
                }
                pos += 3;
            }
            [
                FormulaToken::CellRef(row, col),
                FormulaToken::Separator | FormulaToken::RParen,
                ..,
            ] => {
                if let Some(n) = aggregate_cell(table, *row, *col) {
                    add(n);
                }
                pos += 1;
            }
            _ => {
                let (n, end) = eval_expression(tokens, pos, table, bad)?;
                add(n);
                pos = end;
            }
        }
        match tokens.get(pos)? {
            FormulaToken::Separator => pos += 1,
            FormulaToken::RParen => break,
            _ => return None,
        }
    }
    let result = match function {
        Aggregate::Count => count as f64,
        Aggregate::Average if count == 0 => return None,
        Aggregate::Average => value / count as f64,
        Aggregate::Median => median(&mut median_values)?,
        _ => value,
    };
    Some((result, pos + 1))
}

fn median(values: &mut [f64]) -> Option<f64> {
    let len = values.len();
    if len == 0 {
        return None;
    }
    // Selection is linear and need not sort the entire input range.
    let (lower, upper_middle, _) = values.select_nth_unstable_by(len / 2, f64::total_cmp);
    if len.is_multiple_of(2) {
        let lower_middle = lower.iter().copied().max_by(f64::total_cmp)?;
        Some(lower_middle.midpoint(*upper_middle))
    } else {
        Some(*upper_middle)
    }
}
