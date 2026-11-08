# `web_search`

Search the web and return the top results as `{title, url, snippet}`
triples. This is the discovery half of "compare a row against the live
web": a product name or lot title goes in, candidate pages come out, and
[`web_fetch`](web_fetch.md) reads each one.

The backend is a public search engine's server-rendered HTML page -
no API key and no JavaScript, but also no guarantee the result shape
stays stable forever. Redirect-wrapped result links are decoded so the
returned URLs point at the real pages.

## Parameters

| Parameter     | Type   | Required | Description                                          |
|---------------|--------|----------|------------------------------------------------------|
| `query`       | string | yes      | The search text - a product name, lot title, any query. |
| `max_results` | number | no       | Maximum results returned. Default 5, hard cap 10.    |

## Output

| Field     | Type   | Description                                |
|-----------|--------|--------------------------------------------|
| `query`   | string | The query that ran                         |
| `count`   | number | How many results were parsed               |
| `results` | array  | `{title, url, snippet}` per result         |

## Example

```json
{ "query": "Festool TS 55 used price", "max_results": 5 }
```

```json
{
  "query": "Festool TS 55 used price",
  "count": 5,
  "results": [
    {
      "title": "Festool TS 55 REBQ-Plus ...",
      "url": "https://www.example.com/listing/123",
      "snippet": "Festool TS 55 REBQ-Plus plunge saw, used, EUR 260."
    }
  ]
}
```

## Notes

- **Read-only**: searches and fetches a search page; nothing is written.
- Results are whatever the engine returns for a server-side request -
  ranking and volume differ from a logged-in browser.
- Pair with `web_fetch` to read a result page, or with `enrich_rows`
  (GUI assistant) to enrich a column of links in bulk.
