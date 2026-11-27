//! Reuse one fixed paired training sample for a forecast preview or fill.

use octa::data::{
    CellValue, DataTable, FormulaBadCell, FormulaOutcome, evaluate_formula_with_diagnostics,
};

use super::{FormulaResults, FormulaRows, formula_lex};

// The evaluator also uses the correlation and covariance entry points.
#[allow(dead_code, reason = "shared with direct worksheet evaluation")]
#[path = "../../data/formula_pair_math.rs"]
mod math;

use math::{Block, PairedMoments};

fn fixed_cell(input: &str) -> Option<(usize, usize)> {
    let input = input.trim_ascii();
    let input = input.strip_prefix('$').unwrap_or(input);
    let col_end = input.bytes().take_while(u8::is_ascii_alphabetic).count();
    let (col, row) = input.split_at(col_end);
    let row = row.strip_prefix('$')?;
    if !row.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some((
        row.parse::<usize>().ok()?.checked_sub(1)?,
        formula_lex::column_index(col.as_bytes())?,
    ))
}

fn fixed_range(input: &str) -> Option<Block> {
    let Some((first, last)) = input.split_once(':') else {
        let cell = fixed_cell(input)?;
        return Block::through(cell, cell);
    };
    Block::through(fixed_cell(first)?, fixed_cell(last)?)
}

pub(super) struct Forecast<'formula, 'table> {
    query: FormulaRows<'formula>,
    table: &'table DataTable,
    moments: Option<PairedMoments>,
    scratch: String,
}

impl<'formula, 'table> Forecast<'formula, 'table> {
    pub(super) fn prepare(formula: &'formula str, table: &'table DataTable) -> Option<Self> {
        Self::with_fit(formula, table, math::fit)
    }

    fn with_fit(
        formula: &'formula str,
        table: &'table DataTable,
        fit: impl FnOnce(&DataTable, Block, Block) -> Option<PairedMoments>,
    ) -> Option<Self> {
        let (name, args) = formula.trim_ascii().split_once('(')?;
        if !math::is_forecast(name.trim_ascii_end()) {
            return None;
        }
        // The last two arguments are bare fixed rectangles, so they can't
        // contain separators. Split from the right to leave the query
        // expression (even a nested IF) intact for the usual evaluator.
        let mut args = args.strip_suffix(')')?.rsplitn(3, [',', ';']);
        let x = fixed_range(args.next()?)?;
        let y = fixed_range(args.next()?)?;
        let query = FormulaRows::new(args.next()?.trim_ascii());
        if query.relative.is_empty() {
            return None; // the fixed-form path already evaluates this once
        }
        Some(Self {
            scratch: String::with_capacity(query.source.len()),
            query,
            table,
            moments: fit(table, y, x),
        })
    }

    pub(super) fn at(&mut self, row: usize) -> FormulaOutcome {
        let Some(query) = self.query.at(row, &mut self.scratch) else {
            return FormulaOutcome {
                value: None,
                bad_cell: None,
            };
        };
        // Query coercion and diagnostics stay identical to a single-cell
        // formula, even if fitting the sample did not produce a model.
        let outcome = evaluate_formula_with_diagnostics(query, self.table);
        FormulaOutcome {
            value: outcome
                .value
                .and_then(|x| self.moments.as_ref()?.estimate(x)),
            bad_cell: outcome.bad_cell,
        }
    }
}

pub(super) fn fill(
    formula: &str,
    table: &DataTable,
) -> Option<(FormulaResults, Option<FormulaBadCell>)> {
    let mut forecast = Forecast::prepare(formula, table)?;
    let mut results = Vec::new();
    let mut first_bad = None;
    for row in 0..table.row_count() {
        let outcome = forecast.at(row);
        first_bad = first_bad.or(outcome.bad_cell);
        if let Some(value) = outcome.value {
            results.push((row, value));
        }
    }
    Some((results, first_bad))
}

#[cfg(test)]
mod tests {
    use super::*;
    use octa::data::ColumnInfo;

    fn table() -> DataTable {
        use CellValue::{Int, Null, String as Text};
        DataTable {
            columns: ["X1", "X2", "Y1", "Y2", "Query", "Constant"]
                .map(|name| ColumnInfo {
                    name: name.into(),
                    data_type: "Int64".into(),
                })
                .to_vec(),
            rows: vec![
                vec![Int(0), Int(1), Int(2), Int(5), Int(0), Int(1)],
                vec![Int(2), Null, Int(8), Int(100), Int(-4), Int(1)],
                vec![
                    Int(3),
                    Int(4),
                    Text("missing".into()),
                    Int(14),
                    Text("7".into()),
                    Int(1),
                ],
                vec![
                    Int(5),
                    Int(6),
                    Int(17),
                    Int(20),
                    Text("invalid".into()),
                    Int(1),
                ],
            ],
            ..DataTable::empty()
        }
    }

    fn check_diagnostics(a: Option<FormulaBadCell>, b: Option<FormulaBadCell>) {
        let parts = |v: FormulaBadCell| (v.row, v.col, v.content);
        assert_eq!(a.map(parts), b.map(parts));
    }

    #[test]
    fn batch_fits_once_for_reversed_rectangles_and_preserves_nested_query_diagnostics() {
        let table = table();
        let formula = "PROGNOSE.LINEAR(WENN(ABS(E1)>0;ABS(E1);0);$D$4:$C$1;$B$4:$A$1)";
        let template = FormulaRows::new(formula);
        let mut fits = 0;
        let mut batch = Forecast::with_fit(formula, &table, |table, y, x| {
            fits += 1;
            math::fit(table, y, x)
        })
        .unwrap();
        let mut shifted = String::new();
        for (row, expected) in [Some(2.0), Some(14.0), Some(23.0), None]
            .into_iter()
            .enumerate()
        {
            let direct =
                evaluate_formula_with_diagnostics(template.at(row, &mut shifted).unwrap(), &table);
            let outcome = batch.at(row);
            assert_eq!(outcome.value, direct.value);
            match expected {
                Some(want) => assert!((outcome.value.unwrap() - want).abs() < 1e-13),
                None => assert_eq!(
                    outcome.bad_cell.as_ref().map(|v| (v.row, v.col)),
                    Some((3, 4))
                ),
            }
            check_diagnostics(outcome.bad_cell, direct.bad_cell);
        }
        assert_eq!(fits, 1);
    }

    #[test]
    fn batch_keeps_query_error_if_training_is_degenerate_and_leaves_moving_ranges_to_parser() {
        let table = table();
        for formula in [
            "FORECAST.LINEAR(E1,$D$1:$D$4,$F$1:$F$4)", // constant x series
            "FORECAST.LINEAR(E1,$D$1:$D$3,$F$1:$F$4)", // mismatched shapes
            "FORECAST.LINEAR(E1,$D$500:$D$900,$A$500:$A$900)", // unloaded sample
        ] {
            let (results, bad) = fill(formula, &table).unwrap();
            assert!(results.is_empty(), "{formula}");
            assert_eq!(
                bad.map(|v| (v.row, v.col, v.content)),
                Some((3, 4, "invalid".into()))
            );
        }
        for fallback in [
            "FORECAST.LINEAR(E1,$D1:$D$4,$A$1:$A$4)",
            "IFERROR(FORECAST.LINEAR(E1,$D$1:$D$4,$A$1:$A$4),0)",
            "FORECAST.LINEAR(E1,$D$1:$D$4,$A$1:$A$4)+1",
            "FORECAST.LINEAR(42,$D$1:$D$4,$A$1:$A$4)",
        ] {
            assert!(fill(fallback, &table).is_none(), "{fallback}");
        }
    }
}
