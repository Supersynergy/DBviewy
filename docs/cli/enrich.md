# `--enrich`

Fetch each cell of a column as a URL - or search the web for it - and write
what comes back into a new column. This is the CLI surface of the app's
"Enrich from web" and shares its pipeline: bounded-parallel fetches,
per-host limits and cooldowns, a six-hour page cache.

```
octa --enrich FILE --enrich-on COL [--enrich-target NAME] \
     [--enrich-extract status|text|pattern|search] [--enrich-pattern RE] \
     [--enrich-limit N] [--enrich-top-k N] [--enrich-cell-chars N] \
     [--enrich-provenance] [--enrich-refresh] [--enrich-only-missing] \
     [-f tsv|json|csv]
```

## How it works

`--enrich-on` names the source column (name or index). With the default
`--enrich-extract status` every cell holding a URL is fetched and its HTTP
status lands in `--enrich-target` (default `<on>_live`) - "does the listing
still exist". The other modes keep page content:

| Extract | What is written |
|---|---|
| `status` | The HTTP code, e.g. `200` or `404`. |
| `text` | Readable page text, capped by `--enrich-cell-chars` (default 500). |
| `pattern` | The first capture group of `--enrich-pattern` in the page text. |
| `search` | The cell is a query: the top hit's page is mined (`pattern` when given, text otherwise). `--enrich-top-k` (1-5) tries further hits until one matches the pattern. |

Options:

- `--enrich-limit N` - at most N rows (default 50, cap 500).
- `--enrich-provenance` - also write `<target>_src` (the page a value came
  from, redirects resolved) and `<target>_at` (when it was fetched).
- `--enrich-refresh` - ignore the six-hour page cache (still updated).
- `--enrich-only-missing` - only fetch rows whose target cell is empty, so
  a re-run tops up a partially failed batch without re-fetching answers.

Pages answering 4xx/5xx are skipped as content sources (their "page gone"
body is not a price); status mode still records the code. The fetched
table goes to stdout; a one-line summary and the first failures go to
stderr.

## Examples

```
octa --enrich lots.csv --enrich-on url -f csv > live.csv
octa --enrich lots.csv --enrich-on url --enrich-extract pattern \
     --enrich-pattern '([0-9]+) Lose' --enrich-provenance
octa --enrich lots.csv --enrich-on title --enrich-extract search \
     --enrich-pattern '([0-9]+[,.][0-9]+) ?€' --enrich-top-k 3 \
     --enrich-target live_price --enrich-provenance
```

## See also

- [Enrich from web](../usage/web-enrichment.md) (GUI and in-app assistant).
- [`web_fetch`](../mcp/tools/web_fetch.md) and
  [`web_search`](../mcp/tools/web_search.md) - the read-only pieces.
