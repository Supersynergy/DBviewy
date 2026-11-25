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
| Cell reference  | `A1`, `AA17`, `$B$1` | Column letter and 1-based row number. A dollar sign before the row holds it fixed when filling down. |
| Operators       | `+`, `-`, `*`, `/`, `^`, `%` | Arithmetic, powers and postfix percent. |
| Parentheses     | `(A1 + B1) * 2`     | Override precedence.                                                                             |
| Numeric literal | `42`, `3.14`, `-5`, `2.5E-3` | Integers, decimals and scientific notation with `E` or `e`. |
| Whitespace | `= A1 + B1` | Allowed between tokens, not within cell addresses or numbers. |

Start a formula with `=` in the cell editor or the Insert Column dialog.

## Percentages and powers

`=A1*(1-15%)` applies a 15% discount. The percent operator divides by
100; it does not change the cell's display format. Use `^` or
`POWER(base, exponent)` (`POTENZ`) for powers.

DBviewy applies signs first, then percent, then powers. Chained powers
evaluate from left to right: `2^3^2` is 64. Use parentheses to make a
different order explicit: `-(2^2)` is -4. A numeric literal such as
`1E-3` means 0.001 and is unchanged when you fill a formula column.

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
| `=ROUND(A1*$B$1,2)` | `ROUND(A1*$B$1,2)` | `ROUND(A5*$B$1,2)` |
| `=SUM(A$1:A1)` | `SUM(A$1:A1)` | `SUM(A$1:A5)` |

Use a fixed row for a shared input such as the exchange rate in B1.
`B$1` and `$B$1` both keep row 1 fixed. With `$B1` only the column is
fixed, so the row still advances. Insert Column fills down and never changes
column letters. It looks up all references in the table **before** inserting
the result, even when you place the new column first.

For a running sum such as `SUM(A$1:A1)`, DBviewy keeps the previous total
and reads only the newly included cells. The range must stay in one column
and start at a fixed row. To order or restart totals by another column, use
**Columns > Calculated column > Running total** instead.

## Aggregate functions

`SUM`, `AVERAGE`, `MIN`, `MAX`, `COUNT` and `MEDIAN` accept rectangular ranges, single cells and arithmetic expressions. You can nest functions and separate arguments with commas or semicolons. The German names `SUMME`, `MITTELWERT` and `ANZAHL` also work.

In a range or a single-cell argument, these functions include numbers and numeric text, and skip blanks and other values. `COUNT` returns the number of included cells and scalar arguments. `MEDIAN` skips non-finite values and averages the two middle numbers when there is an even count. When there are no numbers, `AVERAGE` and `MEDIAN` return no numeric result; the other four functions return zero.

### Conditional totals, counts and averages

| Formula | Result |
|----------|--------|
| `=COUNTIF(A1:A100,"West")` | Count rows whose region is West. |
| `=SUMIF(A1:A100,"West",B1:B100)` | Add the amounts for West. |
| `=AVERAGEIF(B1:B100,">="&D1)` | Average amounts at or above the threshold in D1. |

Use a number, a cell reference or quoted text as the criterion. Text
matching ignores case. In text criteria, `*` matches any text and `?`
matches one character. Use `~*` or `~?` for a literal wildcard and
double a quote inside a literal, as in `"A1""blue"`. Numeric comparisons
include numeric text. Use `""` for blank cells, `"<>"` for nonblank cells,
and unquoted `TRUE` or `FALSE` for boolean cells.

If you supply a value range, its top-left cell is where the sum or average
starts. The size and shape come from the criterion range, so prefer matching
rectangles to make the mapping clear. Only loaded cells are considered.
`AVERAGEIF` returns no number if none of the matching values are numeric.
The German names `ZÄHLENWENN`, `SUMMEWENN` and `MITTELWERTWENN` also work.

### Require several conditions at once

| Formula | Result |
|---------|--------|
| `=COUNTIFS(A1:A100,"West",B1:B100,"Paid")` | Count paid records in West. |
| `=SUMIFS(C1:C100,A1:A100,"West",B1:B100,"Paid")` | Total their amounts from column C. |
| `=AVERAGEIFS(C1:C100,A1:A100,"West",C1:C100,">="&D1)` | Average West amounts at or above the threshold in D1. |

Every condition must match. Each range must have the same height and
width, but ranges may start in different places. A single cell is a
one-cell range. Use up to 127 range/criterion pairs. For `SUMIFS` and
`AVERAGEIFS`, put the value range first, unlike the singular forms.

Only positions loaded in every range are considered. `COUNTIFS` and
`SUMIFS` return zero if nothing matches; `AVERAGEIFS` returns no number
if no matching values are numeric. The German names are
`ZÄHLENWENNS`, `SUMMEWENNS` and `MITTELWERTWENNS`.

### Weighted totals and averages

Use `=SUMPRODUCT(B1:B100,C1:C100)` to multiply each unit price in B
by its quantity in C and add the line amounts. Divide by
`SUM(C1:C100)` for the weighted unit price. Wrap the division in
`IFERROR` if a zero total quantity needs a fallback.

Give every range the same height and width. DBviewy pairs cells by
position within their ranges, counts numeric text as a number, and
treats blanks and other text as zero. Only positions loaded in every
range are used. A scalar expression is a one-cell argument. Pass
the ranges as separate arguments rather than multiplying ranges
inside the call. The German name is `SUMMENPRODUKT`.

## Single-value functions

- `ABS(value)` returns the magnitude without the sign.
- `ROUND(value, places)`, also `RUNDEN`, rounds to a whole number of
  decimal places. Use a negative number of places to round to the left
  of the decimal point. For example, `ROUND(125, -1)` returns `130`.
  Halfway values round away from zero, so `ROUND(-1.475, 2)` returns `-1.48`.

## Report by date

- `=YEAR(A1)`, `=MONTH(A1)` and `=DAY(A1)` extract parts of an invoice date.
- `=DAYS(DATE(2024,3,31),A1)` counts calendar days from the invoice date
  to 31 March. It returns a negative number if the invoice comes after that date.
- `=WEEKDAY(A1,2)` returns 1 for Monday through 7 for Sunday. Omit the
  second argument for Sunday as 1; use `3` for Monday as 0. Modes
  `11` to `17` number from 1 starting on Monday through Sunday, respectively.

These functions accept date and timestamp cells, ISO date text
(`YYYY-MM-DD`) and numeric Excel 1900 serials. They ignore time of day
and use the displayed calendar date for a timestamp with an offset.
`DATE` returns a number, not a formatted date. It normalises overflowing
months and days: `DATE(2025,1,0)` refers to 31 December 2024.

Use four-digit years. Values from 0 to 1899 have 1900 added to them.
DBviewy uses the Gregorian calendar: it rejects numeric serial 60, the
fictional 29 February 1900. Day differences crossing that boundary and
weekdays before March 1900 follow the real calendar, not Excel's legacy
behaviour. Raw serials from the 1904 system are not supported.

German names: `DATUM`, `JAHR`, `MONAT`, `TAG`, `TAGE` and `WOCHENTAG`.

## Choose a result or handle a calculation error

- `=IF(A1="West",B1*0.9,B1)` applies a discount to West only.
  Compare numbers or text with `=`, `<>`, `<`, `<=`, `>` or `>=`.
  Text comparison ignores case. A numeric test is true when it is nonzero,
  and a missing false result is zero. The German name is `WENN`.
- `=IFERROR(B1/C1,0)` uses zero when the division cannot be calculated.
  The German name is `WENNFEHLER`. Choose the fallback deliberately: it
  replaces the error rather than correcting the input.

An `IF` evaluates only the selected result. `IFERROR` evaluates the
fallback only if the first calculation fails. Every expression must
still be syntactically valid. Results are numeric; use **Conditional
column** to assign text labels based on rules.

### Combine tests and choose a tier

- `=IF(AND(A1>=100,B1="Paid"),A1*10%,0)` calculates commission only
  for paid orders of at least 100.
- `OR` succeeds when at least one test is true, and `NOT` reverses one
  test. `XOR` succeeds when an odd number of tests are true.
- `=IFS(A1>=500,20,A1>=100,10,TRUE,0)` takes the result for the first
  true test. Use a final `TRUE` pair for a default. Without a match
  there is no numeric result.

`AND`, `OR`, `NOT` and `XOR` return 1 for true or 0 for false, and
evaluate every supplied test. For example, `AND(FALSE,1/0)` still
reports an error. `AND`, `OR` and `XOR` can also take cell ranges:
numbers and booleans count, but text and blank cells are ignored.
If there are no usable inputs, they return no number.

In DBviewy `IFS` does not calculate later tests or unused results.
Their syntax still has to be valid. Give each test a result; an
explicitly empty result, as in `IFS(TRUE,)`, is zero. The German
names are `UND`, `ODER`, `NICHT`, `XODER` and `WENNS`. Use
`WAHR` or `FALSCH` in place of `TRUE` or `FALSE`.

## Look up a value or position

- `=XLOOKUP(D1,$A$1:$A$100,$B$1:$B$100,-1)` finds the ID in D1 and
  returns its price from column B. It uses `-1` only if the ID is not
  found. An omitted fallback returns no number when there is no match.
- `=XMATCH(D1,$A$1:$A$100)` returns the ID's position from the start
  of the search range, starting at 1. An absent match returns no number.

Search one row or column at a time. For `XLOOKUP`, give the return range
the same shape and length as the search range. The result and fallback
are numeric, including numeric text from a matched cell. Non-numeric
matched cells cause an error instead of using the fallback. Lookups do
not spill into other cells.

Text matches ignore case, but text IDs, numbers and booleans remain
distinct. Search starts at the first loaded cell by default. To prefer
the last match, pass `-1` as the last argument:
`=XLOOKUP(D1,A1:A100,B1:B100,-1,0,-1)` or `=XMATCH(D1,A1:A100,0,-1)`.
Match mode `0` is exact. Mode `2` enables text wildcards `*` and `?`
with `~` to escape them. The German names are `XVERWEIS` and
`XVERGLEICH`.

### Find a numeric threshold in a large catalogue

Match mode `-1` takes the exact or next smaller numeric key; mode
`1` takes the exact or next larger one. For example,
`=XLOOKUP(D1,A1:A100,B1:B100,0,-1)` returns the rate at the highest
threshold not above D1, or zero if there is none. The default search
works with unsorted keys and skips non-numeric keys in these modes.

For sorted numeric keys, use search mode `2` for ascending order or
`-2` for descending order. For example,
`=XMATCH(D1,A1:A100000,0,2)` finds a numeric ID without scanning the
whole range. This also works with match mode `-1` or `1` and returns
the first entry when the chosen key repeats. The search does not check
the entire range. **Do not use it with unsorted or mixed-type keys**;
use search mode `1` or `-1` instead. Binary search does not use wildcards.

### Look up a price or tier in a rectangular range

- `=VLOOKUP(D1,$A$1:$C$100,3,FALSE)` searches column A for the ID in D1
  and returns the number from column C, the third column of the range.
- `=HLOOKUP(B4,$A$1:$F$3,3,FALSE)` searches the top row and returns the
  number from the third row of the range, in the same column.

Use `FALSE` or `0` for IDs. It takes the first match and supports `*` and
`?` in text keys; prefix a wildcard with `~` to match it literally.
`TRUE`, `1` or an omitted mode uses ascending numeric thresholds instead.
It chooses the largest threshold not above the key, taking the last entry
if that threshold repeats. Unsorted keys, gaps and non-numeric keys are
not supported in threshold mode. The result is still a single number.
The German names are `SVERWEIS` and `WVERWEIS`.

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
3. **Insert at position**: 1-based column position; the default is to append.
4. **Formula**: start the expression with `=`.

Click **OK** and the new column appears with every row's result.

## Limitations

- For relational lookups, use [Join Tables](join-tables.md) or [SQL](sql.md).
- **No cross-sheet references.** Single table only.
- Formulas return numbers. Text is allowed in comparisons and criteria,
  and `&` joins parts of a criterion. These are not general string operations.
- DBviewy calculates values when you apply the formula. Editing an input
  afterwards does not recalculate already filled cells.

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
