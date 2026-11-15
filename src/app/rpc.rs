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
        "view_page" => view_page(app, &req["params"]).unwrap_or_else(err),
        "set_column_visible" => set_column_visible(app, &req["params"]).unwrap_or_else(err),
        "enrich" => enrich(app, &req["params"]),
        other => err(&format!(
            "unknown method \"{other}\" (ping|tabs|view_page|set_column_visible|enrich)"
        )),
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

/// Read a bounded section of the current filtered grid, without moving focus.
fn view_page(app: &OctaApp, params: &Value) -> Result<String, &'static str> {
    use octa::data::CellValue;
    let tab = app.tabs.get(app.active_tab).ok_or("no active tab")?;
    if tab.filter_dirty {
        return Err("view is updating; retry after the next frame");
    }
    fn integer(params: &Value, name: &str, default: usize) -> Result<usize, &'static str> {
        match params.get(name) {
            None => Ok(default),
            Some(v) => v
                .as_u64()
                .and_then(|v| usize::try_from(v).ok())
                .ok_or("offsets and limits must be non-negative integers"),
        }
    }
    fn excerpt(s: &str) -> String {
        match s.char_indices().nth(128) {
            Some((end, _)) => format!("{}…", &s[..end]),
            None => s.to_owned(),
        }
    }
    let offset = integer(params, "offset", 0)?;
    let column_offset = integer(params, "column_offset", 0)?;
    let limit = integer(params, "limit", 10)?;
    let width = integer(params, "columns", 8)?;
    if !(1..=32).contains(&limit) || !(1..=16).contains(&width) {
        return Err("limit must be 1..32 and columns 1..16");
    }
    let mut visible = tab
        .table
        .columns
        .iter()
        .enumerate()
        .filter(|(i, _)| !tab.hidden_columns.contains(i))
        .skip(column_offset);
    let cols: Vec<_> = visible.by_ref().take(width).collect();
    let next_col = visible.next().map(|_| column_offset + cols.len());
    let headings: Vec<_> = cols
        .iter()
        .map(|(i, c)| json!({"index":i,"name":excerpt(&c.name),"column_key":blake3::hash(c.name.as_bytes()).to_hex().as_str()}))
        .collect();
    let rows: Vec<_> = tab
        .filtered_rows
        .iter()
        .skip(offset)
        .take(limit)
        .map(|&r| {
            let cells: Vec<_> = cols
                .iter()
                .map(|(c, _)| match tab.table.get(r, *c) {
                    None | Some(CellValue::Null) => Value::Null,
                    Some(
                        CellValue::String(s)
                        | CellValue::Date(s)
                        | CellValue::DateTime(s)
                        | CellValue::Nested(s),
                    ) => json!(excerpt(s)),
                    Some(CellValue::Binary(b)) => json!({"bytes":b.len()}),
                    Some(v) => json!(v.to_string()),
                })
                .collect();
            json!({"index":r, "cells":cells})
        })
        .collect();
    let end = offset.saturating_add(rows.len());
    Ok(json!({"ok":true,"tab":format!("#{}",app.active_tab+1),"name":tab_display_name(tab, app.active_tab),
        "columns": headings, "rows": rows, "filtered_loaded_rows":tab.filtered_rows.len(),
        "file_row_offset":tab.table.row_offset, "next_offset":(end < tab.filtered_rows.len()).then_some(end),
        "next_column_offset":next_col}).to_string())
}

/// An idempotent layout change guarded by the tab and column seen by the agent.
fn set_column_visible(app: &mut OctaApp, params: &Value) -> Result<String, &'static str> {
    let idx = app.active_tab;
    let tab = app.tabs.get_mut(idx).ok_or("no active tab")?;
    let handle = format!("#{}", idx + 1);
    if params["tab"].as_str() != Some(&handle)
        || params["name"].as_str() != Some(&tab_display_name(tab, idx))
    {
        return Err("active tab changed; refresh tabs");
    }
    let index = params["column"]
        .as_u64()
        .and_then(|i| usize::try_from(i).ok())
        .ok_or("column index required")?;
    let column = tab.table.columns.get(index).ok_or("column out of range")?;
    if params["column_name"].as_str() != Some(column.name.as_str())
        && params["column_key"].as_str()
            != Some(blake3::hash(column.name.as_bytes()).to_hex().as_str())
    {
        return Err("column changed; refresh the view");
    }
    let visible = params["visible"]
        .as_bool()
        .ok_or("visible must be a boolean")?;
    if !visible
        && !tab.hidden_columns.contains(&index)
        && !octa::ui::table_view::can_hide_column(tab.table.col_count(), &tab.hidden_columns)
    {
        return Err("keep one column visible");
    }
    let changed = if visible {
        tab.hidden_columns.remove(&index)
    } else {
        tab.hidden_columns.insert(index)
    };
    Ok(json!({"ok":true,"changed":changed,"column":index,"visible":visible}).to_string())
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

#[cfg(test)]
mod grid_rpc_tests {
    use super::*;
    use octa::data::{CellValue, ColumnInfo};
    use octa::ui::settings::{AppSettings, IconVariant};

    #[test]
    fn grid_rpc_projection_follows_view_and_layout_is_guarded() {
        let mut app = OctaApp::new(vec![], AppSettings::default(), IconVariant::Rose);
        let t = &mut app.tabs[0];
        t.table.columns = ["public", "internal"]
            .map(|n| ColumnInfo {
                name: n.into(),
                data_type: "Utf8".into(),
            })
            .into();
        t.table.rows = vec![
            vec![CellValue::String("first".into()), CellValue::Null],
            vec![CellValue::String("ü".repeat(150)), CellValue::Null],
        ];
        t.filtered_rows = vec![1, 0];
        t.filter_dirty = false;
        let mut args = json!({"tab":"#1","name":tab_display_name(t,0),"column":1,"column_name":"internal","visible":false});
        for changed in [true, false] {
            let reply: Value =
                serde_json::from_str(&set_column_visible(&mut app, &args).unwrap()).unwrap();
            assert_eq!(reply["changed"], changed);
        }
        let page: Value =
            serde_json::from_str(&view_page(&app, &json!({"limit":1})).unwrap()).unwrap();
        assert_eq!(page["rows"][0]["index"], 1);
        assert_eq!(page["rows"][0]["cells"][0], format!("{}…", "ü".repeat(128)));
        assert_eq!(page["columns"].as_array().unwrap().len(), 1);
        assert_eq!(page["next_offset"], 1);
        args["column_name"] = json!("wrong");
        assert!(set_column_visible(&mut app, &args).is_err());
        args["column_name"] = json!("internal");
        args["visible"] = json!(true);
        assert!(set_column_visible(&mut app, &args).is_ok());
        app.tabs[0].table.columns[1].name = "Ü".repeat(200);
        let page: Value = serde_json::from_str(&view_page(&app, &json!({})).unwrap()).unwrap();
        args["column_key"] = page["columns"][1]["column_key"].clone();
        args["column_name"] = Value::Null;
        args["visible"] = json!(false);
        assert!(set_column_visible(&mut app, &args).is_ok());
        assert!(view_page(&app, &json!({"limit":100000})).is_err());
    }
}
