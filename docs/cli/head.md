# `octa --head`

Print the first N rows of a file, the way Unix `head` does for text
files, except Octa understands the binary formats too. Default
**20 rows**.

## Synopsis

```bash
octa --head FILE [-n N] [--sort-by COLS [--sort-desc COLS] [--natural]] [-f tsv|json|csv]
```

| Flag                | Default | Meaning                                                         |
|---------------------|---------|-----------------------------------------------------------------|
| `-n N`, `--lines N` | `20`    | Number of rows to print. Must be ≥ 0.                           |
| `-f`, `--format`    | `tsv`   | Output format (see [CLI overview](index.md#output-formatting)). |

Sorting flags (also on [`--convert`](convert.md)):

| Flag               | Default       | Meaning                                                                          |
|--------------------|---------------|----------------------------------------------------------------------------------|
| `--sort-by COLS`   | not sorted    | Sort by these columns, first key first. Comma-separated or repeated.             |
| `--sort-desc COLS` | all ascending | The `--sort-by` columns that sort high to low. Each must also be in `--sort-by`. |
| `--natural`        | letter order  | Sort text naturally, `file2` before `file10`.                                    |

## Examples

### Default: first 20 rows as TSV

```bash
$ octa --head sales.parquet
region   quarter  amount   order_id
EU       Q1       1245.50  10001
EU       Q1       89.00    10002
US       Q1       2100.00  10003
... (17 more rows)
```

### Custom row count

```bash
octa --head sales.csv -n 5             # first 5 rows
octa --head sales.csv -n 1             # just the first row (useful as a sample)
octa --head sales.csv -n 1000          # first 1000 rows
```

### The top rows by a column

```bash
octa --head prices.csv --sort-by price --sort-desc price -n 10
```

The rows are sorted first and the first N are taken from the sorted
rows, so this is the ten highest prices. Only the rows that were read
are sorted, up to the row cap (2 million by default); when the file is
longer, a note on stderr says so, and `--rows all` sorts all of it. A
column name that the file does not have is refused with a message naming
it.

### Natural order

```bash
octa --head files.csv --sort-by name --natural
```

Without `--natural` text sorts letter by letter (`file10` before
`file2`), whatever the app's own text-order setting says. Scripts get
the same answer on every machine unless they ask otherwise.

### JSON output for downstream tools

```bash
$ octa --head sales.parquet -n 3 -f json
[
  {
    "region": "EU",
    "quarter": "Q1",
    "amount": 1245.50,
    "order_id": 10001
  },
  {
    "region": "EU",
    "quarter": "Q1",
    "amount": 89.00,
    "order_id": 10002
  },
  {
    "region": "US",
    "quarter": "Q1",
    "amount": 2100.00,
    "order_id": 10003
  }
]
```

### CSV

```bash
$ octa --head sales.parquet -n 2 -f csv
region,quarter,amount,order_id
EU,Q1,1245.5,10001
EU,Q1,89.0,10002
```

Numbers preserve their declared precision in CSV / TSV; in JSON
they keep their JSON-native types (`number`, `boolean`, etc.).

## Raw CSV for agents

Use `octa-csv slice sales.csv 0 20` to emit the header and the first 20
records. The start offset is zero-based. `octa-csv count sales.csv`
prints the number of data records. Both accept `-` for standard input.
These commands read comma-separated files with a header and consistent
field counts. They preserve field bytes, including leading zeroes and
embedded newlines. Later offsets scan from the start by default. For repeated
queries on a local file, create its index with `qsv index sales.csv`.
Then `octa-csv --indexed slice sales.csv 900000 20` seeks directly to
the requested record, and `octa-csv --indexed count sales.csv` reads
the stored count. Supply an index built from that exact file. Rebuild
it after changing or replacing the CSV. To check every recorded offset
against a full CSV parse, run `octa-csv verify-index sales.csv`. It prints
the data record count on success and exits with an error on a mismatch. On macOS the
executable is inside `Octa.app/Contents/MacOS/`.

## Quick Excel preview

Run `octa-xlsx sales.xlsx 20` to print the first populated row and up to 20 following rows as CSV. Supply a worksheet name as the last argument to select another sheet. Column A stays at position one, and the preview width follows the cells read. Workbook metadata and shared strings load first. The reader decodes one cell beyond the requested rows to locate the end of the preview. Formulas use their saved values and number formats are not applied. On macOS the command is in `Octa.app/Contents/MacOS/`.

## Performance

For streaming formats (Parquet, CSV, TSV), Octa loads the standard
**initial-load row cap** (2 Million rows by default, override with
`--rows N|all`), then truncates to N. So `octa --head huge.parquet
-n 10` is fast because the reader itself stops early via the cap;
the actual truncation is just a vector chop. Parquet files with
very many row groups fall back to a DuckDB-backed reader
automatically.

Sorting reads the loaded rows, up to that same cap, and sorts them
before taking the first N. So a sorted `--head` of a file bigger than
the cap sorts only the rows that were loaded: add `--rows all` to sort
a whole large file.

For non-streaming formats (Excel, SQLite, JSON, etc.) the whole
table is loaded into memory and N rows are sliced off the top.

## Notes

- The output **does not include a row count**; pipe through
  `wc -l` to count, or use
  [`octa --sql FILE -q 'SELECT count(*) FROM data'`](sql.md).
- `-n 0` prints just the header row.
- For multi-table sources (SQLite, DuckDB), Octa loads the first
  table; the MCP server's `list_tables` tool gives you discovery
  of the others.
- TAB and newline characters in cells are replaced with spaces in
  TSV output (TSV has no escape mechanism). For lossless output,
  use `-f csv` or `-f json`.

## See also

- [`octa --schema`](schema.md) prints column types alone.
- [`octa --sql`](sql.md) runs `SELECT * FROM data LIMIT N` for the
  same effect with full SQL flexibility.
- [MCP `read_table` tool](../mcp/tools/read_table.md) is the same
  access pattern via MCP.
- MCP: [`read_table`](../mcp/tools/read_table.md), [`count_rows`](../mcp/tools/count_rows.md)
