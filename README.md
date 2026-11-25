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

- **[Open](https://thorstenfoltz.github.io/octa/guide/open/)** files bigger
  than memory, password-protected files and archives, with tabs, tiling and a
  folder sidebar, plus [views](https://thorstenfoltz.github.io/octa/guide/view/)
  for raw text, JSON trees, maps and timelines.
- **[Connect](https://thorstenfoltz.github.io/octa/guide/connect/)** to S3,
  Azure, GCS, SFTP, WebDAV, twelve database engines and REST APIs.
- **[Edit](https://thorstenfoltz.github.io/octa/guide/edit/)** cells, rows and
  columns with undo, formulas and calculated columns, then
  **[clean](https://thorstenfoltz.github.io/octa/guide/clean/)** duplicates,
  outliers, missing values and personal data.
- **[Combine](https://thorstenfoltz.github.io/octa/guide/combine/)** and
  **[compare](https://thorstenfoltz.github.io/octa/guide/compare/)** tables:
  joins (fuzzy and spatial too), file and database diffs, schema drift, cell
  history in Git.
- **[Analyse](https://thorstenfoltz.github.io/octa/guide/analyse/)** with
  summaries, pivots, correlations, time series, cohorts, charts, forecasts and
  HTML reports.
- **[Query](https://thorstenfoltz.github.io/octa/guide/sql-assistant/)** any
  open table with DuckDB SQL, or ask the built-in assistant, on a cloud model or
  offline with [Ollama](https://ollama.com/).
- **[Save](https://thorstenfoltz.github.io/octa/guide/save/)** in the original
  format or convert to another one, and
  **[automate](https://thorstenfoltz.github.io/octa/guide/automate/)** it all
  with recipes, the [command line](https://thorstenfoltz.github.io/octa/cli/)
  or the [MCP server](https://thorstenfoltz.github.io/octa/mcp/).

## What it reads

Parquet, CSV/TSV, JSON and JSON Lines, Excel, ODS, Arrow, Avro, ORC, SQLite,
DuckDB, GeoPackage, Microsoft Access, SAS, QlikView QVD, SPSS, Stata, R, HDF5,
MATLAB, NetCDF, NumPy, MessagePack, BSON, DBF, XML, TOML, YAML, Jupyter
notebooks, Markdown, HTML, EPUB, PDF tables, log files, GeoJSON, Shapefile, GPX,
KML, Delta Lake, Apache Iceberg, Lance, fixed-width text, zip/tar archives, SQL
dumps, email (`.eml`, `.mbox`, Outlook `.msg`, `.pst`, `.olm`), calendars and
contacts, and source code. See
[supported formats](https://thorstenfoltz.github.io/octa/getting-started/supported-formats/)
for what each one can do, and [`samples/`](samples/) for an example of each.

## Installation

Building from source works on all three platforms: install
[Rust](https://rustup.rs/), clone this repository, and run
`cargo build --release`. Linux and macOS need a few native libraries first,
Windows needs a C++ toolchain; the
[installation guide](https://thorstenfoltz.github.io/octa/getting-started/installation/)
lists them per platform, along with uninstall steps and the first-launch
security prompts on Windows and macOS.

Run the desktop app with `cargo run --release --bin octa`. The CLI executable retains the name `octa`.

### Windows

Install from the
[Microsoft Store](https://apps.microsoft.com/detail/9PF9BVRT9PX4): one click, no
SmartScreen prompt, and the Store keeps it updated.

Or download `octa.exe` from the
[releases page](https://github.com/thorstenfoltz/octa/releases) and run it, no
installation needed. `install.ps1` (per-user, no admin) or `install.bat`
(system-wide) set it up properly with a Start Menu entry.

Those downloads are not code-signed, so Windows shows *"Windows protected your
PC"* on first launch. It is safe to continue:
[how to get past the SmartScreen prompt](https://thorstenfoltz.github.io/octa/troubleshooting/#windows-smartscreen-prompt).

### Docker

A headless image ships the CLI and the MCP server:

```bash
docker pull ghcr.io/thorstenfoltz/octa:latest
```

Mount a directory and pass any CLI flag:

```bash
docker run --rm -v "$PWD:/data" ghcr.io/thorstenfoltz/octa --schema /data/file.parquet
```

See the [container docs](https://thorstenfoltz.github.io/octa/cli/docker/) for
the MCP invocation and Podman.

## License

[MIT](LICENSE)
