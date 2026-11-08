//! Answers to `octa --rpc '<json>'` while the app is running.
//!
//! A request arrives over the `single_instance` channel (same port + token
//! the file hand-over uses) and lands here on the UI thread, so the reply
//! is built from real tab state. Replies are one-line JSON: `{"ok":...}`
//! or `{"ok":false,"error":...}`.
//!
//! Methods:
//! - `ping`   - is the app there; name + version.
//! - `tabs`   - every open tab: index, name, rows, columns, read-only.
//! - `enrich` - start an enrich run on an open tab, same guards and the
//!   same status-bar pill as the column-header dialog. Params:
//!   `tab` ("@active" default, `#N`, name or file name), `column` (name or
//!   index), `target`, `extract` (status|text|pattern|search), `pattern`,
//!   `limit`, `top_k`, `cell_chars`, `refresh`, `only_missing`. Provenance
//!   columns are always written - an unattended run must be auditable.

use serde_json::{Value, json};

use crate::app::OctaApp;
use crate::app::chat_panel::helpers::tab_display_name;
use crate::mcp::tools::edit_open_tab::ColRef;
use crate::mcp::tools::enrich_rows::{Extract, Params};

/// Handle one RPC request line and produce the reply line.
pub(crate) fn handle(app: &mut OctaApp, line: &str) -> String {
    let req: Value = match serde_json::from_str(line.trim()) {
        Ok(v) => v,
        Err(e) => return err(&format!("not JSON: {e}")),
    };
    match req["method"].as_str().unwrap_or("") {
        "ping" => {
            json!({"ok": true, "app": "octa", "version": env!("CARGO_PKG_VERSION")}).to_string()
        }
        "tabs" => tabs(app),
        "enrich" => enrich(app, &req["params"]),
        other => err(&format!("unknown method \"{other}\" (ping|tabs|enrich)")),
    }
}

fn err(msg: &str) -> String {
    json!({"ok": false, "error": msg}).to_string()
}

fn tabs(app: &OctaApp) -> String {
    let list: Vec<Value> = app
        .tabs
        .iter()
        .enumerate()
        .map(|(i, t)| {
            json!({
                "index": i,
                "handle": format!("#{}", i + 1),
                "name": tab_display_name(t, i),
                "rows": t.table.row_count(),
                "columns": t.table.col_count(),
                "read_only": t.large.is_some() || t.db_origin.is_some(),
                "active": i == app.active_tab,
            })
        })
        .collect();
    json!({"ok": true, "tabs": list, "enrich_running": app.enrich_run.is_some()}).to_string()
}

fn enrich(app: &mut OctaApp, params: &Value) -> String {
    if let Some(reason) = app.enrich_block_reason() {
        return err(reason);
    }
    let get = |k: &str| params[k].as_str();
    let column = match get("column") {
        Some(c) => c,
        None => return err("enrich needs params.column (name or index)"),
    };
    let extract = match get("extract").unwrap_or("status") {
        "status" => Extract::Status,
        "text" => Extract::Text,
        "pattern" => Extract::Pattern,
        "search" => Extract::Search,
        other => {
            return err(&format!(
                "unknown extract \"{other}\" (status|text|pattern|search)"
            ));
        }
    };
    let url_column = column
        .parse::<usize>()
        .map(ColRef::Index)
        .unwrap_or_else(|_| ColRef::Name(column.to_string()));
    let p = Params {
        open_tab: get("tab").unwrap_or("@active").to_string(),
        url_column,
        target_column: get("target").unwrap_or("").to_string(),
        extract,
        pattern: get("pattern").map(|s| s.to_string()),
        limit: params["limit"].as_u64().map(|n| n as usize),
        cell_chars: params["cell_chars"].as_u64().map(|n| n as usize),
        provenance: true,
        refresh: params["refresh"].as_bool().unwrap_or(false),
        only_missing: params["only_missing"].as_bool().unwrap_or(false),
        top_k: params["top_k"].as_u64().map(|n| n as usize),
    };
    if p.target_column.is_empty() {
        return err("enrich needs params.target (the column to write)");
    }
    app.launch_enrich(p);
    json!({"ok": true, "started": true}).to_string()
}
