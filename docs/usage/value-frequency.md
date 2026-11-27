# Value Frequency

The Value Frequency dialog answers "what are the most common values in
this column?". It is a one-click equivalent of pandas'
`df['x'].value_counts()` for the active table.

![Value Frequency](../assets/screenshots/value-frequency-overview.png)

## Opening the dialog

| Path                          | Notes                                                                                                                                                   |
|-------------------------------|---------------------------------------------------------------------------------------------------------------------------------------------------------|
| **Right-click column header** | "Value frequency…" entry in the column-header context menu.                                                                                             |
| **Analyse menu**              | **Analyse → Value frequency…** opens a column picker first (the menu has no column context), then the dialog for the chosen column.                     |
| **Keyboard shortcut**         | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>I</kbd> Targets the column of the currently selected cell. With no cell selected it opens the same column picker. |

## What it shows

Three columns per row:

| Column    | Meaning                                                                |
|-----------|------------------------------------------------------------------------|
| **Value** | The distinct cell value (or the range, if numeric binning is enabled). |
| **Count** | How many rows have this value (or fall in this bin).                   |
| **%**     | `count / total_non_null * 100`, to one decimal place. The bar below it uses the same 0-100% scale. |

Switching from **All** to **Top 20** does not change the share of an
individual value. For word counts, the share is the percentage of
non-empty cells containing that word or pair, so the shares can add up to
more than 100%. The dialog keeps the exact percentage beside the bar.

For raw value counts, rows are sorted by **Count** descending; ties
broken alphabetically by **Value** for a deterministic ordering. (When
[numeric binning](#numeric-binning-histogram) is on, rows are instead
shown in ascending range order.) The footer reports total distinct
values, total non-null cells, and the null count for the column.

## Top-N presets

| Preset                                              | Effect                                                                      |
|-----------------------------------------------------|-----------------------------------------------------------------------------|
| **Top 20** / **Top 50** / **Top 100** / **Top 500** | Show the N most common values. Default is **Top 50**.                       |
| **All**                                             | Show every distinct value. Watch the row count on high-cardinality columns. |

The selected preset persists per tab, so reopening the dialog on the
same tab remembers your choice. The result table keeps its headers as
you scroll and lays out only the visible rows. Top-N applies to **raw value counts
only** and is hidden while numeric binning is on, where the bin count
controls how many rows you get.

## Numeric binning (histogram)

For numeric columns (`Int*`, `Float*`), check **Bin numeric values**
to build a **histogram**: instead of counting each raw value, Octa
takes the column's range `[min, max]`, splits it into **N equal-width
ranges**, and counts how many rows fall into each.

```
width = (max - min) / N
```

Type a number into the **Bins:** field to set **N** (clamped to
`[1, 1000]`); leave it empty for an automatic count via Sturges' rule
(`ceil(1 + log2(n))`, clamped to `[5, 30]`).

What you see:

- **N bins means N rows.** Every range is shown in ascending order,
  **including empty ones** (count `0`), so the row count is always the
  bin count you asked for and the distribution's shape is visible.
- Range labels are `[lo, hi)` half-open intervals (`lo` included, `hi`
  excluded) so each row counts in exactly one bin; the **last** bin is
  closed `[lo, hi]` so the maximum value has a home.
- The **Count** column is how many rows fall in that range; tall counts
  mark where your data clusters.
- When every value in the column is identical there's no range to split,
  so you get a single bucket.

Because the bin count is the control while binning, the **Top-N**
presets (Top 20 / 50 / … / All) are hidden; they apply only to raw
value counts. Non-finite values (`NaN`, `±Inf`) and accidental
non-numeric cells in a numeric column appear as separate rows after the
bins, useful for catching type drift.

The checkbox is hidden for non-numeric columns. Untick it to go back to
exact per-value counts.

## Words inside text

Whole cells say little about a column of comments, ticket texts or product
names, where almost every cell is different. For a text column, set
**Count:** to **Words** to count the words inside the cells, or to **Word
pairs** to count pairs of neighbouring words (`late delivery`), which tells
`not good` from `good`.

- Words are compared in lower case and split at anything that is not a
  letter or a digit, so `Late,` and `late` are one word. An apostrophe
  inside a word stays (`don't`).
- A word counts **once per cell**, however often the cell repeats it. So
  **Count** is the number of rows that hold the word and the percentage is
  its share of the rows that hold any word at all.
- A cell without a word (empty, or punctuation only) counts as null. With
  **Word pairs** a one-word cell has no pair and counts as null too.
- Text written without spaces, such as Chinese, Japanese or Thai, is not
  split into words: a sentence there counts as one word.
- Words are counted in the rows the tab holds. On a database tab or a
  partly loaded file the footer says so.

An example, a column with three comments:

| comment              |
|----------------------|
| Late delivery, again |
| late delivery        |
| Delivery was late    |

**Words** counts `late` 3, `delivery` 3, `again` 1 and `was` 1. **Word pairs**
counts `late delivery` 2, and `delivery again`, `delivery was` and `was late`
1 each, so the complaint that keeps coming back stands at the top.

**Top-N** and **Copy as TSV** work as for whole cells. **Filter table to
this value** is greyed, since a word is not a cell value; use the search
box to find the rows. **Count:** is greyed on a number column.

## Acting on a frequency row

Right-click any row (when binning is off) to get:

- **Copy value** puts the raw value on the clipboard.
- **Filter table to this value** adds a column filter restricting the
  active table to rows where this column equals the picked value. The
  Excel-style column-filter chip appears in the status bar; see
  [Column Filter](search-and-filter.md#column-filter) to clear it.

## Copy as TSV

**Copy as TSV** copies the current report, including rows outside the
viewport. A Top-N report copies only its top N. The three columns are
`<column>`, `count` and `percent`. Tabs, quotes and line breaks in
a value stay inside its field.

## Open the report as a table

**Open frequency report as table** says how many rows it will open. It
copies that report to a separate tab, not to the source. With **Top 20**,
it opens no more than 20 rows. Use **All** first when you need every value,
for example to save the complete result as XLSX or Parquet.

The `value` column stays text, so a code such as `0007` keeps its zeros.
`count` is an integer and `percent` is a number on a 0-100 scale.
The tab carries any caveat about counting only loaded rows or excluding
unsaved database edits.

A report of whole-cell values adds `cumulative_percent`: the running
share of the original counted cells in the report's order. For example,
categories with shares of 50, 30 and 20 reach 50, 80 and 100. A Top-N
report can finish below 100. Word counts may overlap, and numeric bins
are ordered by interval, so neither gets this column.

To create a [Pareto chart](chart.md#pareto) for the complete distribution,
open the **All** report, then choose **Chart > Pareto**, with `value` as
the category and `count` as the measure. A chart made from only the Top-N
rows describes that subset.

## Command line and MCP

The same job without the GUI:

- MCP: [`value_frequency`](../mcp/tools/value_frequency.md)

## See also

- [Summary](summary.md): one row of statistics per column, for a
  table-wide overview.
- [Search & Filter](search-and-filter.md): including the column
  filter that Value Frequency can populate.
- [Keyboard Shortcuts](../reference/shortcuts.md): to rebind
  Ctrl+Shift+I.
