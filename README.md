<p align="center">
  <img src="assets/dbviewy.svg" alt="DBviewy database and search logo" width="128" height="128">
</p>

<h1 align="center">DBviewy</h1>

<p align="center"><strong>Open, search and edit tables on your desktop.</strong></p>

<p align="center">
  <a href="#build-from-source">Build from source</a> ·
  <a href="#workbook-safety">Workbook safety</a> ·
  <a href="CHANGELOG.md">Changelog</a> ·
  <a href="LICENSE">MIT license</a>
</p>

DBviewy is a native fork of [Octa](https://github.com/thorstenfoltz/octa), based
on Octa 0.22.1. Open an Excel workbook, CSV file, SQLite database or Parquet
file and work with it in a tabbed grid. Run DuckDB SQL on open tables without
moving them to another app.

## What DBviewy adds

| Workflow | How it works |
| --- | --- |
| **Find rows while you type** | Press `Ctrl+F` to focus instant search. Use the options menu to switch between Filter and Highlight, or limit the search to a column. `Esc` clears the query. |
| **Edit from the grid or ribbon** | Enter a cell address in the name box to jump there. Use the formula bar to edit the selected cell, or collapse the ribbon for more table space. |
| **Work across sheets and tables** | Open workbooks from Finder on macOS. Choose an individual sheet or database table, or open all tables from a local SQLite or DuckDB database. Opening all tables loads them into memory. |
| **Restore columns one at a time** | Reopen a folded column from its header without restoring the other folded columns. |
| **Keep pictures with the data** | Store thumbnails in a portable SQLite catalogue and browse them without a separate image folder. |
| **Ask with text or voice** | Smartchat uses the same draft as the assistant panel. DBviewy adds a SYC Free profile and optional local Parakeet dictation. |

The grid, SQL, file readers, database and cloud connectors, charts, CLI and MCP
server come from Octa. See the [upstream guide](https://thorstenfoltz.github.io/octa/)
for their documentation. Upstream downloads install **Octa**, not DBviewy.

## Workbook safety

For supported cell-value edits to an existing `.xlsx` file, DBviewy patches
the worksheet in the original workbook. Other sheets, formulas, styles and
filters stay in place. Excel may recalculate formulas after reopening; DBviewy
does not promise live recalculation of the whole workbook.

Unsupported edits, such as changes to formula cells or the workbook structure,
are refused by this save path. Use a separate export when you need a different
table or file format. A filtered table export is not a copy of the full
workbook.

## Build from source

You need Git LFS for the bundled fonts and data, and Rust via
[rustup](https://rustup.rs/). See Octa's
[platform dependencies](https://thorstenfoltz.github.io/octa/getting-started/installation/)
for macOS, Windows and Linux.

```sh
git clone https://github.com/Supersynergy/DBviewy.git
cd DBviewy
git lfs pull
cargo run --release --locked --bin octa
```

To open a file directly, append `-- path/to/file.xlsx` to the last command.
The binary is still named `octa` for compatibility.

For changes in this fork, see the [changelog](CHANGELOG.md). For the original
project and contributors, see [Octa on GitHub](https://github.com/thorstenfoltz/octa)
and the [notices](NOTICE).
