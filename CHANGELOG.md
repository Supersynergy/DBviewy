# Changelog

## Unreleased

- Share one vector logo between the README, desktop app and package icons.
- Document DBviewy workflows and the limits of in-place workbook saves.
- Fill blank cells in existing XLSX rows without exporting a new workbook.
- Find prices or numeric tiers with `VLOOKUP` and `HLOOKUP` (German:
  `SVERWEIS` and `WVERWEIS`).
- Find the next smaller or larger numeric key with `XLOOKUP` and `XMATCH`.
  Use binary search when the numeric keys are sorted.
- Calculate invoice ages and weekday groups with spreadsheet date functions.
  Preserve imported XLSX dates as dates when exporting the report.
- Keep tabs, line breaks and quotes inside cells when copying and pasting tables.
- Paste and undo large row blocks without repeatedly rebuilding existing edits.
- Look up a value by row and column with `INDEX` and `MATCH` (`VERGLEICH`).
- Count workdays and calculate deadlines with holidays and custom weekends.
- Reuse totals for repeated criteria when filling a formula column over fixed ranges.
- Count or total invoices by date range, including XLSX timestamps on the last day.
- Correct existing XLSX dates and timestamps without changing the cell format or date system.
- Save clock-time and duration corrections in the original XLSX without changing their number formats.
- Calculate shift hours across midnight with `TIMEVALUE` and `MOD`, or extract clock parts.
- Find minimum and maximum amounts with `MINIFS` and `MAXIFS`, including clock and duration criteria.
- Preview formula results in Insert Column without changing the table.
- Find formula examples by English or German name and use a selected input column.
- Convert imported number text with explicit decimal and grouping separators.
- Calculate percentiles and quartiles, including p95 response times, with inclusive and exclusive Excel formulas.
- Measure sample and population spread with standard deviation and variance, including running columns.
- Reveal the correct row when navigating filtered tables with zoomed or resized rows.
- Fill down typed values within the visible selection, with one Undo.
- Fill typed values to the right without changing hidden or unselected cells.
- Preview Fill series by column and refuse to write if the target has changed.
- Keep selection statistics compact and exclude hidden rows and columns.
- Check data completeness with `COUNTA`, `COUNTBLANK` and `ISBLANK`, also in German.
- Fill running averages and counts without rescanning earlier rows.

- Integrate Octa 0.22.1, keeping DBviewy's workbook preservation, per-session
  crash markers, assistant integrations, and local table interface.

- Rebuild the menu row: no sideways scrolling at normal widths (a hidden wheel
  scroll remains only for very narrow windows), instant search directly after
  the menus, and one search-options menu instead of eight loose controls.
- Add a collapsible Excel-style editing bar (setting `show_ribbon`, default on):
  quick actions that reuse the menu actions, a name box with cell jump, and a
  formula bar that commits through the same path as an in-cell edit.
- Extract `commit_cell_text` from the table cell editor so the formula bar and
  the cell editor share one parser.
- Add `ROUND`, `ABS` and `MEDIAN` to cell and column formulas, including
  negative rounding precision and decimal ties away from zero.
- Keep fixed Excel-style references such as `$B$1` when filling a calculated
  column. Evaluate against the original table regardless of insertion position.
- Fill single-column running sums such as `SUM(A$1:A1)` in one pass instead
  of rescanning the full range for each row.
- Add `COUNTIF`, `SUMIF` and `AVERAGEIF` with text, number, blank and wildcard
  criteria. Keep quoted identifiers unchanged when filling formula columns.
- Add `IF` and `IFERROR` for numeric formulas. Check both branches for
  syntax errors without reading cells from the unused branch.
- Add `COUNTIFS`, `SUMIFS` and `AVERAGEIFS` to count, sum or average
  cells that meet every criterion.
- Look up a numeric value with `XLOOKUP` or find a position with `XMATCH`.
  Use exact or wildcard matching and choose the first or last result.
- Calculate weighted totals from corresponding cells with `SUMPRODUCT`.
- Support percent, powers and scientific notation in formulas. Keep
  exponents such as the one in `1E-3` fixed when filling a column.
- Combine tests with `AND`, `OR`, `NOT` and `XOR`, and select the first
  matching result with `IFS`.
- Clear existing XLSX cell values without losing their style or changing
  other workbook parts. Refill a cleared final row from the same tab.
- Build the macOS bundle without a Homebrew runtime dependency and check
  all three bundled executables before signing.

- Fix the native macOS About, Hide and Quit labels without moving settings
  or session storage. Load the current DBviewy icon directly in native About.
- Align the local binary and bundle version, and use a dedicated icon resource
  name so AppKit cannot reuse an older octopus icon after an update.

- Centre instant search between the scrolling menus and Smartchat.
- Replace the oversized welcome logo with open actions, twelve recent files,
  file sizes and paths, shortcuts and build details.
- Add Open all tables to the database picker, with Ctrl+Enter support.

- Give DBviewy a navy database and turquoise search-lens logo, distinct from Octa.
- Keep instant search visible outside the scrolling menus, with Ctrl+F focus,
  immediate search updates, Escape and a clear button. Preserve Ask, search
  options and the separate Smartchat draft.
- Document DBviewy additions separately from inherited Octa features.
