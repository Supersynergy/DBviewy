# Enrich from web

Columns that hold a URL or a searchable title per row - product pages,
listing links, source references - can be fetched in bulk without
leaving the grid.

## How it works

Right-click a column header holding usable text and pick **Enrich from
web...**. A dialog asks what to extract and where it should land, then a
background worker fetches up to 500 rows, a polite handful per host at a
time.

The **Extract** choices:

- **HTTP status** - the page's status code; `200` for a live listing,
  `404` for a gone one, an error note where the fetch itself failed.
- **Page text** - readable page text (HTML converted), capped per cell.
- **Regex pattern** - keeps the first capture group of the first regex
  match on each page; the pattern field is required here.
- **Search top hits** - treats each cell as a search query and fetches
  the top result pages instead of a URL. A title column becomes live
  prices or availability in one run.

The dialog also sets the **target column** (created when missing,
overwritten when it exists), the **row cap**, the **cell char cap** for
text extraction, how many **top hits** a search tries per row (with a
pattern the first matching hit wins; candidates sharing words with the
query are tried first), the **provenance columns** toggle, **Ignore
cache** for a fresh pass, and **Only fill empty cells** so a re-run
tops up the gaps without re-fetching reviewed values.

Rows are processed in the order you see them: a sorted or filtered view
means the cap picks up the first *visible* rows and rows the filter hid
are skipped. Pages answering 4xx/5xx never become values - a "this page
is gone" body is not a price - and a host shedding load (429/503) earns
a short cooldown instead of burning the rest of the batch into failures.

While the run is in flight the status bar shows a live counter and a
**Cancel** button; cancelling stops new pages and keeps the values
already fetched.

## What lands where

Nothing is written silently. The whole run arrives as one batch in the
[Edit Audit Trail](edit-audit-trail.md), where you review every value
before it counts, and a single undo removes the run. With provenance on,
two extra columns travel with it: `<target>_src` records the URL the
value actually came from (redirects followed), `<target>_at` the fetch
time - so an audit entry can always answer "where did this 200 come
from, and when".

Pages already fetched inside the last six hours come from a local cache
(`fetch_cache.json` in the config dir) instead of the network; failed
fetches are never cached. A cached value keeps its original fetch time
in `<target>_at`, so the audit trail shows the answer's real age rather
than stamping it "now".

## Guards

The entry refuses politely instead of surprising you: with Write
protection on, on a [read-only](editing.md#read-only-mode) tab, or on a
[large-file](large-files.md) window. Fetching is capped and bounded so a
right-click can never turn into an uncontrolled crawl.

## Richer extracts

The same four modes drive the [Assistant](chatbot.md)'s `enrich_rows`
tool, which shares the same queue and audit trail - handy when a
chat-driven run should batch the same extraction. `web_search` finds
candidate pages by hand before `web_fetch` or `enrich_rows` reads them.

On the command line, [`--enrich`](../cli/enrich.md) runs the same pipeline
over a file and prints the enriched table to stdout - no app needed:

```
octa --enrich lots.csv --enrich-on url --enrich-provenance > live.csv
```

In the app: column header right-click, **Enrich from web...**.
