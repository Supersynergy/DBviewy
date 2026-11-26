//! Excel-like formula evaluation against a [`DataTable`].
//!
//! Supports cell references (e.g. `A1`, `BC42`), numeric literals, the four
//! arithmetic operators, parenthesised sub-expressions, scalar functions and
//! numeric aggregates over ranges.
//! Re-exported from [`crate::data`] so call sites keep their existing import
//! path (`use octa::data::evaluate_formula`).

use std::cmp::Ordering;

use super::{CellValue, DataTable};

#[path = "formulas_control.rs"]
mod control;
#[path = "formulas_criteria.rs"]
mod criteria;
#[path = "formulas_date.rs"]
mod date;
#[path = "formula_lex.rs"]
mod lex;
#[path = "formulas_lookup.rs"]
mod lookup;
#[path = "formulas_product.rs"]
mod product;
use criteria::ConditionalAggregate;

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
    let mut context = Evaluation {
        table: Some(table),
        bad: None,
    };
    let value = eval_complete(&tokens, &mut context);
    FormulaOutcome {
        value,
        bad_cell: context.bad,
    }
}

/// Without a table, parse the same grammar but do not read cells or check
/// runtime arithmetic. Lazy functions use this for the inactive expression.
#[derive(Default)]
struct Evaluation<'a> {
    table: Option<&'a DataTable>,
    bad: Option<FormulaBadCell>,
}

fn eval_complete(tokens: &[FormulaToken], context: &mut Evaluation<'_>) -> Option<f64> {
    let (value, end) = eval_expression(tokens, 0, context)?;
    (end == tokens.len()).then_some(value)
}

/// Resolve the two parts of a reference after removing any anchors.
fn parse_cell_ref(letters: &[u8], digits: &str) -> Option<(usize, usize)> {
    let mut col: usize = 0;
    for &letter in letters {
        if !letter.is_ascii_alphabetic() {
            return None;
        }
        col = col
            .checked_mul(26)?
            .checked_add(usize::from(letter.to_ascii_uppercase() - b'A' + 1))?;
    }
    let row: usize = digits.parse().ok()?;
    Some((row.checked_sub(1)?, col.checked_sub(1)?))
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
    Text(String),
    Boolean(bool),
    CellRef(usize, usize), // (row, col)
    Op(char),              // +, -, *, /, ^, %
    Compare(Comparison),
    Function(Function),
    Range,
    Separator,
    Concat,
    LParen,
    RParen,
}

#[derive(Clone, Copy, Debug)]
enum Function {
    Aggregate(Aggregate),
    Conditional(ConditionalAggregate),
    ConditionalMany(ConditionalAggregate),
    Abs,
    Round,
    If,
    IfError,
    Ifs,
    Logical(control::Logical),
    Lookup(lookup::ResultKind),
    TableLookup(lookup::Axis),
    Index,
    Match,
    Calendar(date::Calendar),
    SumProduct,
    Power,
}

#[derive(Clone, Copy, Debug)]
enum Comparison {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl Comparison {
    fn matches(self, order: Ordering) -> bool {
        match self {
            Self::Eq => order.is_eq(),
            Self::Ne => !order.is_eq(),
            Self::Lt => order.is_lt(),
            Self::Le => !order.is_gt(),
            Self::Gt => order.is_gt(),
            Self::Ge => !order.is_lt(),
        }
    }
}

fn function_named(name: &str) -> Option<Function> {
    use Aggregate::{Average, Count, Max, Median, Min, Sum};
    use Function::{Abs, Round};
    const NAMES: &[(&str, Function)] = &[
        ("SUM", Function::Aggregate(Sum)),
        ("SUMME", Function::Aggregate(Sum)),
        ("AVERAGE", Function::Aggregate(Average)),
        ("MITTELWERT", Function::Aggregate(Average)),
        ("MIN", Function::Aggregate(Min)),
        ("MAX", Function::Aggregate(Max)),
        ("COUNT", Function::Aggregate(Count)),
        ("ANZAHL", Function::Aggregate(Count)),
        ("MEDIAN", Function::Aggregate(Median)),
        (
            "COUNTIF",
            Function::Conditional(ConditionalAggregate::Count),
        ),
        (
            "ZÄHLENWENN",
            Function::Conditional(ConditionalAggregate::Count),
        ),
        (
            "Zählenwenn",
            Function::Conditional(ConditionalAggregate::Count),
        ),
        ("SUMIF", Function::Conditional(ConditionalAggregate::Sum)),
        (
            "SUMMEWENN",
            Function::Conditional(ConditionalAggregate::Sum),
        ),
        (
            "AVERAGEIF",
            Function::Conditional(ConditionalAggregate::Average),
        ),
        (
            "MITTELWERTWENN",
            Function::Conditional(ConditionalAggregate::Average),
        ),
        ("ABS", Abs),
        ("ROUND", Round),
        ("RUNDEN", Round),
        ("IF", Function::If),
        ("WENN", Function::If),
        ("IFERROR", Function::IfError),
        ("WENNFEHLER", Function::IfError),
        ("XLOOKUP", Function::Lookup(lookup::ResultKind::Value)),
        ("XVERWEIS", Function::Lookup(lookup::ResultKind::Value)),
        ("XMATCH", Function::Lookup(lookup::ResultKind::Position)),
        ("XVERGLEICH", Function::Lookup(lookup::ResultKind::Position)),
        ("VLOOKUP", Function::TableLookup(lookup::Axis::Column)),
        ("SVERWEIS", Function::TableLookup(lookup::Axis::Column)),
        ("HLOOKUP", Function::TableLookup(lookup::Axis::Row)),
        ("WVERWEIS", Function::TableLookup(lookup::Axis::Row)),
        ("INDEX", Function::Index),
        ("MATCH", Function::Match),
        ("VERGLEICH", Function::Match),
        ("DATE", Function::Calendar(date::Calendar::Date)),
        ("DATUM", Function::Calendar(date::Calendar::Date)),
        ("YEAR", Function::Calendar(date::Calendar::Year)),
        ("JAHR", Function::Calendar(date::Calendar::Year)),
        ("MONTH", Function::Calendar(date::Calendar::Month)),
        ("MONAT", Function::Calendar(date::Calendar::Month)),
        ("DAY", Function::Calendar(date::Calendar::Day)),
        ("TAG", Function::Calendar(date::Calendar::Day)),
        ("DAYS", Function::Calendar(date::Calendar::Days)),
        ("TAGE", Function::Calendar(date::Calendar::Days)),
        ("WEEKDAY", Function::Calendar(date::Calendar::Weekday)),
        ("WOCHENTAG", Function::Calendar(date::Calendar::Weekday)),
        ("SUMPRODUCT", Function::SumProduct),
        ("SUMMENPRODUKT", Function::SumProduct),
        ("POWER", Function::Power),
        ("POTENZ", Function::Power),
        ("IFS", Function::Ifs),
        ("WENNS", Function::Ifs),
        ("AND", Function::Logical(control::Logical::And)),
        ("UND", Function::Logical(control::Logical::And)),
        ("OR", Function::Logical(control::Logical::Or)),
        ("ODER", Function::Logical(control::Logical::Or)),
        ("XOR", Function::Logical(control::Logical::Xor)),
        ("XODER", Function::Logical(control::Logical::Xor)),
        ("NOT", Function::Logical(control::Logical::Not)),
        ("NICHT", Function::Logical(control::Logical::Not)),
        (
            "COUNTIFS",
            Function::ConditionalMany(ConditionalAggregate::Count),
        ),
        (
            "ZÄHLENWENNS",
            Function::ConditionalMany(ConditionalAggregate::Count),
        ),
        (
            "Zählenwenns",
            Function::ConditionalMany(ConditionalAggregate::Count),
        ),
        (
            "SUMIFS",
            Function::ConditionalMany(ConditionalAggregate::Sum),
        ),
        (
            "SUMMEWENNS",
            Function::ConditionalMany(ConditionalAggregate::Sum),
        ),
        (
            "AVERAGEIFS",
            Function::ConditionalMany(ConditionalAggregate::Average),
        ),
        (
            "MITTELWERTWENNS",
            Function::ConditionalMany(ConditionalAggregate::Average),
        ),
    ];
    NAMES
        .iter()
        .find(|(word, _)| word.eq_ignore_ascii_case(name))
        .map(|&(_, function)| function)
}

fn tokenize_formula(expr: &str) -> Option<Vec<FormulaToken>> {
    // Evaluation uses recursive descent for nested parentheses and calls.
    const MAX_NESTING: usize = 64;
    let mut tokens = Vec::new();
    let bytes = expr.as_bytes();
    let mut i = 0;
    let mut depth = 0usize;
    while let Some(&byte) = bytes.get(i) {
        match byte {
            c if c.is_ascii_whitespace() => {
                i += 1;
            }
            b'+' | b'-' | b'*' | b'/' | b'^' | b'%' => {
                tokens.push(FormulaToken::Op(char::from(byte)));
                i += 1;
            }
            b'=' | b'<' | b'>' => {
                let (op, width) = match (byte, bytes.get(i + 1)) {
                    (b'<', Some(b'=')) => (Comparison::Le, 2),
                    (b'>', Some(b'=')) => (Comparison::Ge, 2),
                    (b'<', Some(b'>')) => (Comparison::Ne, 2),
                    (b'<', _) => (Comparison::Lt, 1),
                    (b'>', _) => (Comparison::Gt, 1),
                    _ => (Comparison::Eq, 1),
                };
                tokens.push(FormulaToken::Compare(op));
                i += width;
            }
            b':' => {
                tokens.push(FormulaToken::Range);
                i += 1;
            }
            b',' | b';' => {
                tokens.push(FormulaToken::Separator);
                i += 1;
            }
            b'&' => {
                tokens.push(FormulaToken::Concat);
                i += 1;
            }
            b'"' => {
                i += 1;
                let mut value = String::new();
                loop {
                    let end = i + bytes.get(i..)?.iter().position(|&b| b == b'"')?;
                    value.push_str(&expr[i..end]);
                    i = end + 1;
                    if bytes.get(i) != Some(&b'"') {
                        break;
                    }
                    value.push('"');
                    i += 1;
                }
                tokens.push(FormulaToken::Text(value));
            }
            b'(' => {
                depth += 1;
                if depth > MAX_NESTING {
                    return None;
                }
                tokens.push(FormulaToken::LParen);
                i += 1;
            }
            b')' => {
                depth = depth.checked_sub(1)?;
                tokens.push(FormulaToken::RParen);
                i += 1;
            }
            c if c.is_ascii_digit() || c == b'.' => {
                let start = i;
                i = lex::number_end(bytes, start)?;
                let val: f64 = expr[start..i].parse().ok()?;
                tokens.push(FormulaToken::Number(val));
            }
            c if c.is_ascii_alphabetic() || c == b'$' => {
                let column_fixed = c == b'$';
                i += usize::from(column_fixed);
                let col_start = i;
                // Function names can contain an umlaut (ZÄHLENWENN).
                // Cell columns are still strictly ASCII in parse_cell_ref.
                while bytes
                    .get(i)
                    .is_some_and(|b| b.is_ascii_alphabetic() || !b.is_ascii())
                {
                    i += 1;
                }
                let col_end = i;
                if col_start == col_end {
                    return None;
                }
                let row_fixed = bytes.get(i) == Some(&b'$');
                i += usize::from(row_fixed);
                let row_start = i;
                while bytes.get(i).is_some_and(u8::is_ascii_digit) {
                    i += 1;
                }
                if !column_fixed && !row_fixed && row_start == i {
                    let name = &expr[col_start..col_end];
                    if name.eq_ignore_ascii_case("TRUE") || name.eq_ignore_ascii_case("WAHR") {
                        tokens.push(FormulaToken::Boolean(true));
                    } else if name.eq_ignore_ascii_case("FALSE")
                        || name.eq_ignore_ascii_case("FALSCH")
                    {
                        tokens.push(FormulaToken::Boolean(false));
                    } else {
                        tokens.push(FormulaToken::Function(function_named(name)?));
                    }
                } else {
                    let (row, col) =
                        parse_cell_ref(&bytes[col_start..col_end], &expr[row_start..i])?;
                    tokens.push(FormulaToken::CellRef(row, col));
                }
            }
            _ => return None,
        }
    }
    (depth == 0).then_some(tokens)
}

/// Recursive descent parser: expression = term (('+' | '-') term)*
fn eval_expression(
    tokens: &[FormulaToken],
    pos: usize,
    context: &mut Evaluation<'_>,
) -> Option<(f64, usize)> {
    let (mut left, mut p) = eval_term(tokens, pos, context)?;
    while p < tokens.len() {
        match &tokens[p] {
            FormulaToken::Op('+') => {
                let (right, np) = eval_term(tokens, p + 1, context)?;
                left += right;
                p = np;
            }
            FormulaToken::Op('-') => {
                let (right, np) = eval_term(tokens, p + 1, context)?;
                left -= right;
                p = np;
            }
            _ => break,
        }
    }
    Some((left, p))
}

/// term = power (('*' | '/') power)*
fn eval_term(
    tokens: &[FormulaToken],
    pos: usize,
    context: &mut Evaluation<'_>,
) -> Option<(f64, usize)> {
    let (mut left, mut p) = eval_power(tokens, pos, context)?;
    while p < tokens.len() {
        match &tokens[p] {
            FormulaToken::Op('*') => {
                let (right, np) = eval_power(tokens, p + 1, context)?;
                left *= right;
                p = np;
            }
            FormulaToken::Op('/') => {
                let (right, np) = eval_power(tokens, p + 1, context)?;
                if context.table.is_some() {
                    if right == 0.0 {
                        return None;
                    }
                    left /= right;
                }
                p = np;
            }
            _ => break,
        }
    }
    Some((left, p))
}

/// Excel binds signs and postfix percentages before powers, and follows
/// the same left-to-right rule for chained powers as for other operators.
fn eval_power(
    tokens: &[FormulaToken],
    pos: usize,
    context: &mut Evaluation<'_>,
) -> Option<(f64, usize)> {
    let (mut base, mut end) = eval_percent(tokens, pos, context)?;
    while matches!(tokens.get(end), Some(FormulaToken::Op('^'))) {
        let (exponent, next) = eval_percent(tokens, end + 1, context)?;
        base = numeric_power(base, exponent, context)?;
        end = next;
    }
    Some((base, end))
}

fn eval_percent(
    tokens: &[FormulaToken],
    pos: usize,
    context: &mut Evaluation<'_>,
) -> Option<(f64, usize)> {
    let (mut value, mut end) = eval_factor(tokens, pos, context)?;
    while matches!(tokens.get(end), Some(FormulaToken::Op('%'))) {
        if context.table.is_some() {
            value /= 100.0;
            if !value.is_finite() {
                return None;
            }
        }
        end += 1;
    }
    Some((value, end))
}

fn numeric_power(base: f64, exponent: f64, context: &Evaluation<'_>) -> Option<f64> {
    if context.table.is_none() {
        return Some(0.0); // syntax-only pass of an inactive branch
    }
    if !base.is_finite() || !exponent.is_finite() || (base == 0.0 && exponent == 0.0) {
        return None;
    }
    let value = base.powf(exponent);
    value.is_finite().then_some(value)
}

/// factor = ('+' | '-')* (Number | CellRef | function | '(' expression ')')
fn eval_factor(
    tokens: &[FormulaToken],
    mut pos: usize,
    context: &mut Evaluation<'_>,
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
        FormulaToken::Number(n) => {
            (context.table.is_none() || n.is_finite()).then_some((*n, pos + 1))
        }
        FormulaToken::Boolean(value) => Some((f64::from(u8::from(*value)), pos + 1)),
        FormulaToken::Function(function) => eval_function(*function, tokens, pos + 1, context),
        FormulaToken::CellRef(..) if context.table.is_none() => Some((0.0, pos + 1)),
        FormulaToken::CellRef(row, col) => {
            match cell_as_f64(context.table?, *row, *col) {
                Ok(Some(v)) => Some((v, pos + 1)),
                // Out-of-bounds reference: treat as 0 so a partly-overrun
                // formula still produces a result for the rows that do fit.
                Ok(None) => Some((0.0, pos + 1)),
                Err(b) => {
                    if context.bad.is_none() {
                        context.bad = Some(b);
                    }
                    // Tell the caller "this row can't be numerically
                    // resolved" by aborting the walk.
                    None
                }
            }
        }
        FormulaToken::LParen => {
            let (val, p) = eval_expression(tokens, pos + 1, context)?;
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
    context: &mut Evaluation<'_>,
) -> Option<(f64, usize)> {
    match function {
        Function::Aggregate(aggregate) => {
            return eval_aggregate(aggregate, tokens, pos, context);
        }
        Function::Conditional(aggregate) => {
            return criteria::evaluate(aggregate, tokens, pos, context);
        }
        Function::ConditionalMany(aggregate) => {
            return criteria::evaluate_many(aggregate, tokens, pos, context);
        }
        Function::Lookup(kind) => return lookup::evaluate(kind, tokens, pos, context),
        Function::TableLookup(axis) => return lookup::evaluate_table(axis, tokens, pos, context),
        Function::Index => return lookup::evaluate_index(tokens, pos, context),
        Function::Match => return lookup::evaluate_match(tokens, pos, context),
        Function::Calendar(kind) => return date::evaluate(kind, tokens, pos, context),
        Function::SumProduct => return product::evaluate(tokens, pos, context),
        Function::If => return control::evaluate_if(tokens, pos, context),
        Function::IfError => return control::evaluate_if_error(tokens, pos, context),
        Function::Ifs => return control::evaluate_ifs(tokens, pos, context),
        Function::Logical(op) => return control::evaluate_logical(op, tokens, pos, context),
        _ => {}
    }
    if !matches!(tokens.get(pos), Some(FormulaToken::LParen)) {
        return None;
    }
    let (value, end) = eval_expression(tokens, pos + 1, context)?;
    let (value, end) = match function {
        Function::Abs => (value.abs(), end),
        Function::Round => {
            if !matches!(tokens.get(end), Some(FormulaToken::Separator)) {
                return None;
            }
            let (places, end) = eval_expression(tokens, end + 1, context)?;
            let rounded = if context.table.is_some() {
                round_decimal(value, places)?
            } else {
                0.0
            };
            (rounded, end)
        }
        Function::Power => {
            if !matches!(tokens.get(end), Some(FormulaToken::Separator)) {
                return None;
            }
            let (exponent, end) = eval_expression(tokens, end + 1, context)?;
            (numeric_power(value, exponent, context)?, end)
        }
        _ => unreachable!("aggregates and control functions are handled above"),
    };
    if !matches!(tokens.get(end), Some(FormulaToken::RParen)) {
        return None;
    }
    (context.table.is_none() || value.is_finite()).then_some((value, end + 1))
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
    context: &mut Evaluation<'_>,
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
                if let Some(table) = context.table {
                    let end_row = r1.max(r2).saturating_add(1).min(table.row_count());
                    let end_col = c1.max(c2).saturating_add(1).min(table.col_count());
                    for row in *r1.min(r2)..end_row {
                        for col in *c1.min(c2)..end_col {
                            if let Some(n) = aggregate_cell(table, row, col) {
                                add(n);
                            }
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
                if let Some(n) = context
                    .table
                    .and_then(|table| aggregate_cell(table, *row, *col))
                {
                    add(n);
                }
                pos += 1;
            }
            _ => {
                let (n, end) = eval_expression(tokens, pos, context)?;
                if context.table.is_some() {
                    add(n);
                }
                pos = end;
            }
        }
        match tokens.get(pos)? {
            FormulaToken::Separator => pos += 1,
            FormulaToken::RParen => break,
            _ => return None,
        }
    }
    if context.table.is_none() {
        return Some((0.0, pos + 1));
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
