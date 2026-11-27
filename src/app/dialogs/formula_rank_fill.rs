//! Fast Insert Column path for one moving value in a fixed comparison range.

use octa::data::{DataTable, FormulaBadCell, evaluate_formula_with_diagnostics};

use super::{FormulaResults, FormulaRows, dispersion, formula_lex};

#[path = "../../data/formula_rank_math.rs"]
mod rank_math;

fn reference(input: &str, fixed_row: bool) -> Option<(usize, usize)> {
    let input = input.strip_prefix('$').unwrap_or(input);
    let length = input.bytes().take_while(u8::is_ascii_alphabetic).count();
    let (col, row) = input.split_at(length);
    let digits = if fixed_row {
        row.strip_prefix('$')?
    } else {
        row
    };
    if !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let row = digits.parse::<usize>().ok()?.checked_sub(1)?;
    Some((row, formula_lex::column_index(col.as_bytes())?))
}

pub(super) fn fill(
    formula: &str,
    table: &DataTable,
) -> Option<(FormulaResults, Option<FormulaBadCell>)> {
    let (name, text) = formula.trim().split_once('(')?;
    let kind = rank_math::Kind::named(name.trim_ascii_end())?;
    // Optimise only the unambiguous whole-call form. The evaluator owns
    // all other syntax, including expressions used for the order.
    let mut args = text
        .strip_suffix(')')?
        .split([',', ';'])
        .map(str::trim_ascii);
    let needle = args.next()?;
    reference(needle, false)?;
    let (a, b) = args.next()?.split_once(':')?;
    let (from, col) = reference(a.trim_ascii(), true)?;
    let (to, last_col) = reference(b.trim_ascii(), true)?;
    if col != last_col {
        return None;
    }
    let order = match args.next() {
        Some(text) => text.parse().ok()?,
        None => 0.0,
    };
    let ascending = rank_math::ascending(order)?;
    if args.next().is_some() {
        return None;
    }

    let start = from.min(to);
    let end = (from.max(to) + 1).min(table.row_count());
    let mut sample: Vec<_> = if col < table.col_count() {
        (start..end)
            .filter_map(|row| table.get(row, col).and_then(dispersion::cell_number))
            .filter(|value| value.is_finite())
            .collect()
    } else {
        Vec::new()
    };
    sample.sort_unstable_by(f64::total_cmp);
    let addresses = FormulaRows::new(needle);
    let mut buffer = String::with_capacity(needle.len());
    let mut results = Vec::new();
    let mut first_bad = None;
    for row in 0..table.row_count() {
        let address = addresses.at(row, &mut buffer);
        let Some(address) = address else {
            continue;
        };
        let outcome = evaluate_formula_with_diagnostics(address, table);
        if first_bad.is_none() {
            first_bad = outcome.bad_cell;
        }
        if let Some(needle) = outcome.value.filter(|v| v.is_finite()) {
            // Native < and <= put both spellings of zero in one tie group.
            let low = sample.partition_point(|v| *v < needle);
            let high = sample.partition_point(|v| *v <= needle);
            let ahead = if ascending { low } else { sample.len() - high };
            if let Some(rank) = kind.result(ahead, high - low) {
                results.push((row, rank));
            }
        }
    }
    Some((results, first_bad))
}
