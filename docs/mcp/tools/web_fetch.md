# `web_fetch`

Fetch an `http(s)` URL and return its content as **readable text**. HTML pages
are converted to plain text (scripts, styles and markup dropped, block tags
become line breaks, entities decoded); `raw: true` returns the body verbatim.
Read-only (stays available under `--mcp-read-only`).

## When to use

- "Read the product page this `url` cell points at."
- "Pull the listing text for this lot and compare its price."
- "Fetch a small JSON/text endpoint a cell links to."

Redirect chains are followed (up to 5 hops) and the landing address is
reported in `final_url`. One request is capped at 20 s and 4 MB of body.

## Input schema

| Parameter   | Type    | Required? | Default      | Description                                |
|-------------|---------|-----------|--------------|--------------------------------------------|
| `url`       | string  | yes       | (no default) | `http://` or `https://` address to fetch   |
| `max_chars` | integer | no        | `60000`      | Text cap for the reply (hard max `500000`) |
| `raw`       | bool    | no        | `false`      | Keep the body verbatim (no HTML-to-text)   |

## Response shape

```json
{
  "url": "https://example.com/lot/31",
  "final_url": "https://example.com/en/lot/31",
  "status": 200,
  "content_type": "text/html; charset=utf-8",
  "char_count": 18342,
  "truncated": false,
  "text": "Lot 31 - Workbench lot\nStarting bid 45.00 EUR\n..."
}
```

## Example call

```json
{
  "name": "web_fetch",
  "arguments": {
    "url": "https://auktionen.restlos.com/auktionen/-/1995/Grosse-Lagereinigung-Werkzeug",
    "max_chars": 20000
  }
}
```

## Notes

- `http://`/`https://` only - `file:`, `ftp:` and friends are refused before
  any request goes out.
- The HTML-to-text pass is extraction, not layout: reading order and words
  are kept, positioning and styling are not.
- For JSON APIs the body comes back verbatim (JSON is not "HTML", so `raw`
  changes nothing there).

In the app: the same tool is available to the in-GUI assistant as part of the
`core` tool group.
