# `--rpc`

Send one JSON request to the Octa that is already running and print its
JSON reply. This is how scripts drive the live app - the same guarded,
token-locked loopback channel the single-instance file hand-over uses, so
only your own user account can talk to your Octa.

```
octa --rpc '{"method":"ping"}'
octa --rpc '{"method":"tabs"}'
octa --rpc '{"method":"enrich","params":{"column":"url","target":"live"}}'
```

The reply is one JSON line: `{"ok": ...}` or `{"ok": false, "error": ...}`.
Without a running Octa the command fails politely.

## Methods

| Method | What it does |
|---|---|
| `ping` | Answers `{"ok":true,"app":"octa","version":...}` - a health probe. |
| `tabs` | Lists every open tab: index, `#N` handle, name, rows, columns, whether it is read-only, and whether an enrich run is in progress. |
| `enrich` | Starts an enrich run on an open tab. Same guards, same status-bar progress pill and the same audit-trail review as the in-app dialog - a script cannot bypass Write protection or read-only tabs. |

`enrich` params (inside `"params"`):

| Key | Meaning |
|---|---|
| `tab` | `"@active"` (default), `#N`, a tab name, or a file name. |
| `column` | Source column: name or index. Required. |
| `target` | Column to write. Required. |
| `extract` | `status` (default), `text`, `pattern`, or `search`. |
| `pattern` | Regex for `pattern`/`search` extraction. |
| `limit`, `top_k`, `cell_chars` | Row cap, search hit depth, per-cell text cap. |
| `refresh`, `only_missing` | Ignore the page cache; top up empty cells only. |

Provenance columns (`<target>_src`, `<target>_at`) are always written for
RPC runs - an unattended run must stay auditable.

## See also

- [Enrich from web](../usage/web-enrichment.md) - the app and CLI versions.
- [`--enrich`](enrich.md) - the same pipeline over a file, no running app
  needed.
