# Formulas

Octa supports a small subset of Excel-like formulas in two places:

1. **Inline in a cell**: type `=A1+B1` into any cell and the
   evaluated result replaces the formula on Enter.
2. **In the Insert Column dialog**: a formula entered there is
   treated as a row-1 template and applied to **every row** when the
   column is created.

## Syntax

| Element         | Example             | Notes                                                                                            |
|-----------------|---------------------|--------------------------------------------------------------------------------------------------|
| Cell reference  | `A1`, `B2`, `AA17`  | Column letter (A, B, … Z, AA, AB, …) + 1-based row number. Column letters appear in each header. |
| Operators       | `+`, `-`, `*`, `/`  | Standard precedence; multiplication/division before addition/subtraction.                        |
| Parentheses     | `(A1 + B1) * 2`     | Override precedence.                                                                             |
| Numeric literal | `42`, `3.14`, `-5` | Integers and decimals.                                          |
| Whitespace      | `= A1 + B1`         | Ignored everywhere.                                                                              |

Start a formula with `=` in the cell editor or the Insert Column dialog.

## Examples

```
=A1+B1              # add two columns
=(A1-B1)/B1*100     # percentage change from B1 to A1
=A1*1.05            # add 5%
=AVERAGE(A1:A3)     # average of three cells
=SUM(A1:B2, D1, 5)  # add a rectangle, a single cell and a constant
=ROUND(A1*B1, 2)    # round a line total to cents
=ABS(A1-B1)         # distance between two values
=MEDIAN(B1:B9)      # middle value in a range
```

For Insert Column, the formula is applied per-row:

| Formula   | On row 1 | On row 5 |
|-----------|----------|----------|
| `=A1+B1`  | `A1+B1`  | `A5+B5`  |
| `=A1*1.5` | `A1*1.5` | `A5*1.5` |
| `=B1-C1`  | `B1-C1`  | `B5-C5`  |
| `=SUM(A1:C1)` | `SUM(A1:C1)` | `SUM(A5:C5)` |

## Aggregate functions

`SUM`, `AVERAGE`, `MIN`, `MAX`, `COUNT` and `MEDIAN` accept rectangular ranges, single cells and arithmetic expressions. You can nest functions and separate arguments with commas or semicolons. The German names `SUMME`, `MITTELWERT` and `ANZAHL` also work.

In a range or a single-cell argument, these functions include numbers and numeric text, and skip blanks and other values. `COUNT` returns the number of included cells and scalar arguments. `MEDIAN` skips non-finite values and averages the two middle numbers when there is an even count. When there are no numbers, `AVERAGE` and `MEDIAN` return no numeric result; the other four functions return zero.

## Single-value functions

- `ABS(value)` returns the magnitude without the sign.
- `ROUND(value, places)`, also `RUNDEN`, rounds to a whole number of
  decimal places. Use a negative number of places to round to the left
  of the decimal point. For example, `ROUND(125, -1)` returns `130`.
  Halfway values round away from zero, so `ROUND(-1.475, 2)` returns `-1.48`.

## What gets evaluated

In arithmetic expressions, the engine converts each referenced cell to a number:

- **Numeric columns** (Int*, Float*) are used directly.
- **String columns** are parsed via `parse::<f64>()`. The cell is
  flagged as non-numeric if parsing fails.
- **Null cells** are coerced to `0.0`.
- **Boolean / Date / DateTime / Binary / Nested** are non-numeric;
  surfaced as errors (see below).

The result is rendered as a number with `Display` precision.
Division by zero yields no numeric result. Inline editing keeps the input
as text; Insert Column leaves that result empty.

## When a formula can't be evaluated

If an arithmetic expression references a non-numeric cell (a string that doesn't parse,
a date, a binary blob), the formula short-circuits and the row's
output is empty. For the Insert Column path, a banner appears above
the table:

> *Formula skipped N of M rows; first non-numeric reference: cell
> (row, column) → "...".*

This makes it obvious that some rows didn't get a value, and points
at the first problematic cell so you can fix the column type or
clean the data.

The error short-circuits at the **first** bad cell per row, not per
formula, so fix that one and re-evaluate to see the next one.

## Insert Column with a formula

From [**Columns → Insert column…**](editing.md#inserting-columns):

![Insert column dialog with a formula](../assets/screenshots/insert-column-formula.png)

1. **Name**: the new column's name.
2. **Type**: pick a numeric type (Int64 / Float64) when the formula
   produces numbers.
3. **Insert at position**: 0-based index. Blank = append.
4. **Formula**: start the expression with `=`.

Click **OK** and the new column appears with every row's result.

## Limitations

- For conditional expressions such as `IF` and lookups, use [SQL](sql.md).
- **No cross-sheet references.** Single table only.
- **No string operations.** Formulas are numeric-only.
- **No row references like `$A1` or `A$1`.** Insert Column always
  shifts row numbers and preserves column letters, so write the formula
  for row 1 and it'll be applied to every row.

If your use case bumps against these, the
[SQL panel](sql.md) is almost always the right tool:
DuckDB has the full spectrum of analytical functions plus joins,
group-by, window functions, and more.

## See also

- [SQL panel](sql.md) runs DuckDB queries against the
  loaded table.
- [Date / Time calculation](date-time-calculation.md) derives a new
  column from date, datetime, or duration values.
- [Editing](editing.md) covers how Insert Column dialogs work.
