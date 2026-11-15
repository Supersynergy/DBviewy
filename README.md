# DBviewy

<p align="left">
<img src="assets/octa-rose.svg" alt="DBviewy" width="128" height="128">
</p>

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

DBviewy is a desktop table editor based on [Octa](https://github.com/thorstenfoltz/octa).
Open a workbook from Finder, edit cells and follow domain links directly from the grid.
For lead workbooks, the Smartchat accepts typed requests or local Parakeet dictation
and can switch sheets and filter open reviews by city.

Edit cells with undo and save in the original format, or use Save As to convert the table. DuckDB can query any open table, including CSV files.

📚 **Documentation:** <https://thorstenfoltz.github.io/octa/>

![Open a file](docs/assets/screenshots/first-steps-file-menu.png)

## Contents

- [What it does](#what-it-does)
- [What it reads](#what-it-reads)
- [Installation](#installation)
- [License](#license)

## What it does

The [user guide](https://thorstenfoltz.github.io/octa/guide/) is sorted by task, and each topic lists where a
job lives: in the app, on the command line, and as an MCP tool for AI agents.

- **[Open files](https://thorstenfoltz.github.io/octa/guide/open/)**: tabs with tiling and groups, a folder
  sidebar, files bigger than memory, password-protected files, archives, PDF
  tables, log files, and saved mail, calendars and contacts.
- **[Connect](https://thorstenfoltz.github.io/octa/guide/connect/)**: S3, Azure, GCS, SFTP and WebDAV, twelve
  live database engines from PostgreSQL to BigQuery, and REST APIs.
- **[View and find](https://thorstenfoltz.github.io/octa/guide/view/)**: sorting, search, filters, marks,
  bookmarks, and views for raw text, JSON trees, maps and timelines.
- **[Edit and reshape](https://thorstenfoltz.github.io/octa/guide/edit/)**: inline edits with undo, formulas,
  calculated columns, number bands, type changes, pivot and transpose.
- **[Clean and protect](https://thorstenfoltz.github.io/octa/guide/clean/)**: duplicates, outliers, missing
  values, validation rules, personal data and anonymisation.
- **[Combine and split](https://thorstenfoltz.github.io/octa/guide/combine/)**: union, join, fuzzy and spatial
  joins, key finding, merging edited copies, splitting into files.
- **[Analyse](https://thorstenfoltz.github.io/octa/guide/analyse/)**: summaries, value counts, correlations,
  time series, cohorts, funnels, customer segments, charts, forecasts, seasonality and HTML reports.
- **[Compare and history](https://thorstenfoltz.github.io/octa/guide/compare/)**: file and database diffs,
  schema and data drift, cell history in Git, earlier versions.
- **[Save and export](https://thorstenfoltz.github.io/octa/guide/save/)**: save back, convert, PDF, workbooks
  and DDL.
- **[SQL and assistant](https://thorstenfoltz.github.io/octa/guide/sql-assistant/)**: DuckDB SQL over any open
  file, and a chat assistant that runs on a cloud model or offline with
  [Ollama](https://ollama.com/).
- **[Automate](https://thorstenfoltz.github.io/octa/guide/automate/)**: recipes that replay a clean-up, batch
  conversion, a full [command line](https://thorstenfoltz.github.io/octa/cli/) and an
  [MCP server](https://thorstenfoltz.github.io/octa/mcp/) for Claude and other
  [Model Context Protocol](https://modelcontextprotocol.io/) clients.

## What it reads

Parquet, CSV/TSV, JSON and JSON Lines, Excel, ODS, Arrow, Avro, ORC, SQLite,
DuckDB, GeoPackage, Microsoft Access, SAS (also transport files), QlikView QVD, SPSS, Stata, R, HDF5, MATLAB, NetCDF, NumPy, MessagePack, BSON,
DBF, XML, TOML, YAML, Jupyter notebooks, Markdown, HTML, EPUB, PDF tables, log
files, GeoJSON, Shapefile, GPX, KML, Delta Lake, Apache Iceberg, Lance, fixed-width text, zip/tar
archives, SQL dumps, email and mailboxes (`.eml`, `.mbox`, Outlook `.msg`, `.pst`,
`.olm`), calendars and contacts (`.ics`, `.vcf`), and source code. Many of those
can be written back, and Save As converts between them.

See [supported formats](https://thorstenfoltz.github.io/octa/getting-started/supported-formats/)
for the full list and what each one can do, or open [`samples/`](samples/) for
one small example of every one of them.

## Installation

Building from source works on all three platforms: install
[Rust](https://rustup.rs/), clone this repository, and run
`cargo build --release`. Linux and macOS need a few native libraries first,
Windows needs a C++ toolchain; the
[installation guide](https://thorstenfoltz.github.io/octa/getting-started/installation/)
lists them per platform, along with uninstall steps and the first-launch
security prompts on Windows and macOS.

Run the desktop app with `cargo run --release --bin octa`. The CLI executable retains the name `octa`.

## License

MIT
