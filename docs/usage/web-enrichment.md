# Enrich from web

Columns that hold a URL per row - product pages, listing links, source
references - can be fetched in bulk without leaving the grid.

## How it works

Right-click a column header. When a sampled cell holds a URL, the menu
shows **Enrich from web**. Octa fetches each row's URL on a background
worker (up to 500 rows, several at a time) and writes the HTTP status of
every page into a new `<name>_live` column: `200` for a live listing,
`404` for a gone one, an error note where the fetch itself failed.

## What lands where

Nothing is written silently. The whole run arrives as one batch in the
[Edit Audit Trail](edit-audit-trail.md), where you review every value
before it counts, and a single undo removes the run. The status bar
reports progress and the final tally (values written, pages fetched,
failures).

## Guards

The entry refuses politely instead of surprising you: with Write
protection on, on a [read-only](editing.md#read-only-mode) tab, or on a
[large-file](large-files.md) window. Fetching is capped and bounded so a
right-click can never turn into an uncontrolled crawl.

## Richer extracts

The GUI default answers "is this link alive?". Pulling the page text or a
regex capture - "extract the live price into `live_price`" - is a job for
the [Assistant](chatbot.md)'s `enrich_rows` tool, which shares the same
queue and audit trail. `web_search` finds candidate pages for a title
before `web_fetch` or `enrich_rows` reads them.

In the app: column header right-click, **Enrich from web**.
