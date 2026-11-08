# Enrich from web

Columns that hold a URL per row - product pages, listing links, source
references - can be fetched in bulk without leaving the grid.

## How it works

Right-click a column header. When a sampled cell holds a URL, the menu
shows **Enrich from web**. Octa fetches each row's URL on a background
worker (up to 500 rows, a polite handful per host at a time) and writes
the HTTP status of every page into a new `<name>_live` column: `200` for
a live listing, `404` for a gone one, an error note where the fetch
itself failed.

While the run is in flight the status bar shows a live counter and a
**Cancel** button; cancelling stops new pages and keeps the values
already fetched.

## What lands where

Nothing is written silently. The whole run arrives as one batch in the
[Edit Audit Trail](edit-audit-trail.md), where you review every value
before it counts, and a single undo removes the run. Two provenance
columns travel with it: `<name>_live_src` records the URL the value
actually came from (redirects followed), `<name>_live_at` the fetch time -
so an audit entry can always answer "where did this 200 come from, and
when".

Pages already fetched inside the last six hours come from a local cache
(`fetch_cache.json` in the config dir) instead of the network; failed
fetches are never cached.

## Guards

The entry refuses politely instead of surprising you: with Write
protection on, on a [read-only](editing.md#read-only-mode) tab, or on a
[large-file](large-files.md) window. Fetching is capped and bounded so a
right-click can never turn into an uncontrolled crawl.

## Richer extracts

The GUI default answers "is this link alive?". Pulling the page text or a
regex capture - "extract the live price into `live_price`" - is a job for
the [Assistant](chatbot.md)'s `enrich_rows` tool, which shares the same
queue and audit trail. Its `extract: "search"` mode goes one step
further: a column of titles becomes queries, each row's top result page
is fetched and mined (with `pattern` for a regex capture, else page
text), and `_src` then proves which page the value came from - the
missing link for live price comparisons. `web_search` finds candidate
pages by hand before `web_fetch` or `enrich_rows` reads them.

In the app: column header right-click, **Enrich from web**.
