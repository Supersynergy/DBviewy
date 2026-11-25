# Changelog

## Unreleased

- Share one vector logo between the README, desktop app and package icons.
- Document DBviewy workflows and the limits of in-place workbook saves.

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
