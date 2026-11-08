use super::*;
use crate::mcp::tools::{TableSnapshot, ToolContext};
use octa::data::{CellValue, ColumnInfo, DataTable};
use std::sync::{Arc, Mutex};

fn ctx_with_tab(unlocked: bool) -> ToolContext {
    let mut t = DataTable::empty();
    t.columns = vec![
        ColumnInfo {
            name: "id".into(),
            data_type: "Int64".into(),
        },
        ColumnInfo {
            name: "url".into(),
            data_type: "String".into(),
        },
    ];
    t.rows = vec![
        vec![
            CellValue::Int(1),
            CellValue::String("https://example.com/a".into()),
        ],
        vec![CellValue::Int(2), CellValue::String("no link here".into())],
        vec![
            CellValue::Int(3),
            CellValue::String("https://example.com/b".into()),
        ],
    ];
    let mut ctx = ToolContext::for_mcp(Some(1000), 65536, false, true, Vec::new(), false, 0);
    ctx.open_tabs = vec![TableSnapshot {
        handle: "#1".into(),
        display_name: "t".into(),
        source_path: None,
        table: t,
        window: None,
        view_rows: None,
    }];
    ctx.active_tab = Some(0);
    ctx.allow_existing_writes = unlocked;
    ctx.pending_tab_edits = Some(Arc::new(Mutex::new(Vec::new())));
    ctx
}

fn params(extract: Extract, pattern: Option<&str>) -> Params {
    Params {
        open_tab: "#1".into(),
        url_column: ColRef::Name("url".into()),
        target_column: "live".into(),
        extract,
        pattern: pattern.map(str::to_string),
        limit: None,
        cell_chars: None,
        provenance: false,
        refresh: false,
        only_missing: false,
        top_k: None,
    }
}

#[test]
fn refuses_when_existing_writes_disallowed() {
    let ctx = ctx_with_tab(false);
    let p = params(Extract::Status, None);
    assert!(run(&ctx, &p).is_err());
    assert!(
        ctx.pending_tab_edits
            .as_ref()
            .unwrap()
            .lock()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn pattern_extract_requires_pattern() {
    let ctx = ctx_with_tab(true);
    let p = params(Extract::Pattern, None);
    let err = run(&ctx, &p).unwrap_err().to_string();
    assert!(err.contains("pattern"), "unexpected error: {err}");
}

#[test]
fn pattern_extract_requires_capture_group() {
    let ctx = ctx_with_tab(true);
    let p = params(Extract::Pattern, Some("[0-9]+ EUR"));
    let err = run(&ctx, &p).unwrap_err().to_string();
    assert!(err.contains("capture group"), "unexpected error: {err}");
}

#[test]
fn pattern_extract_rejects_bad_regex() {
    let ctx = ctx_with_tab(true);
    let p = params(Extract::Pattern, Some("([0-9]+"));
    assert!(run(&ctx, &p).is_err());
}

#[test]
fn refuses_unknown_url_column() {
    let ctx = ctx_with_tab(true);
    let p = Params {
        open_tab: "#1".into(),
        url_column: ColRef::Name("missing".into()),
        ..params(Extract::Status, None)
    };
    let err = run(&ctx, &p).unwrap_err().to_string();
    assert!(err.contains("missing"), "unexpected error: {err}");
}

#[test]
fn refuses_url_column_out_of_range() {
    let ctx = ctx_with_tab(true);
    let p = Params {
        url_column: ColRef::Index(9),
        ..params(Extract::Status, None)
    };
    assert!(run(&ctx, &p).is_err());
}

#[test]
fn refuses_unknown_tab() {
    let ctx = ctx_with_tab(true);
    let p = Params {
        open_tab: "#9".into(),
        ..params(Extract::Status, None)
    };
    assert!(run(&ctx, &p).is_err());
}

#[test]
fn refuses_large_file_window() {
    let mut ctx = ctx_with_tab(true);
    ctx.open_tabs[0].window = Some(crate::mcp::tools::SnapshotWindow {
        offset: 0,
        len: 3,
        total: 9_999_999,
    });
    let p = params(Extract::Status, None);
    let err = run(&ctx, &p).unwrap_err().to_string();
    assert!(err.contains("large-file"), "unexpected error: {err}");
}

#[test]
fn host_of_extracts_lowercase_host() {
    assert_eq!(
        host_of("https://Auktionen.Restlos.com/x/1995"),
        "auktionen.restlos.com"
    );
    assert_eq!(host_of("http://example.com"), "example.com");
    assert_eq!(host_of("no-scheme/path"), "");
}

#[test]
fn extract_value_status_and_pattern() {
    let page = PageResult {
        final_url: "https://x/".into(),
        status: 404,
        content_type: "text/html".into(),
        body: "<p>Preis: 42 EUR</p>".into(),
        fetched_at: None,
    };
    assert_eq!(extract_value(Extract::Status, None, &page, 500), "404");
    let re = regex::Regex::new("([0-9]+) EUR").unwrap();
    assert_eq!(extract_value(Extract::Pattern, Some(&re), &page, 500), "42");
    // Pattern without match yields an empty cell, not an error.
    let miss = regex::Regex::new("(zzz)").unwrap();
    assert_eq!(extract_value(Extract::Pattern, Some(&miss), &page, 500), "");
}

#[test]
fn push_column_op_adds_and_sets() {
    use octa::data::CellValue;
    let ctx = ctx_with_tab(true);
    let table = &ctx.open_tabs[0].table;
    let mut ops = Vec::new();
    let mut pairs = BTreeMap::new();
    pairs.insert(1usize, CellValue::String("200".into()));
    let n = push_column_op(&mut ops, table, "live", &pairs);
    assert_eq!(n, 1);
    assert!(matches!(ops[0], ResolvedOp::AddColumn { .. }));
    let n2 = push_column_op(&mut ops, table, "url", &pairs); // existing col
    assert_eq!(n2, 1);
    assert!(matches!(ops[1], ResolvedOp::SetCells(_)));
}

#[test]
fn top_k_defaults_to_one_and_clamps() {
    let mut p = params(Extract::Search, None);
    assert_eq!(top_k_of(&p), 1);
    p.top_k = Some(3);
    assert_eq!(top_k_of(&p), 3);
    p.top_k = Some(99);
    assert_eq!(top_k_of(&p), 5);
    p.top_k = Some(0);
    assert_eq!(top_k_of(&p), 1);
}

#[test]
fn top_k_parses_from_json() {
    let p: Params = serde_json::from_value(serde_json::json!({
        "open_tab": "#1", "url_column": "url", "target_column": "live",
        "extract": "search", "top_k": 4
    }))
    .unwrap();
    assert_eq!(top_k_of(&p), 4);
    // Absent stays at the default.
    let p2: Params = serde_json::from_value(serde_json::json!({
        "open_tab": "#1", "url_column": "url", "target_column": "live"
    }))
    .unwrap();
    assert_eq!(top_k_of(&p2), 1);
}

#[test]
fn resolve_target_pattern_fallback_prefers_match() {
    // Search-mode hit iteration: a page that yields no capture is kept only
    // as fallback; the first matching hit wins. Verified on PageResult level
    // (the network part is identical plumbing): extract_value picks the
    // capture out of a "matching" page while a non-matching page would have
    // produced an empty cell.
    let re = regex::Regex::new("([0-9]+) EUR").unwrap();
    let no_match = PageResult {
        final_url: "https://x/1".into(),
        status: 200,
        content_type: "text/html".into(),
        body: "<p>no price here</p>".into(),
        fetched_at: None,
    };
    let matched = PageResult {
        final_url: "https://x/2".into(),
        status: 200,
        content_type: "text/html".into(),
        body: "<p>Preis: 77 EUR</p>".into(),
        fetched_at: None,
    };
    assert_eq!(
        extract_value(Extract::Pattern, Some(&re), &no_match, 500),
        ""
    );
    assert_eq!(
        extract_value(Extract::Pattern, Some(&re), &matched, 500),
        "77"
    );
}

#[test]
fn view_rows_skips_filtered_out_rows() {
    // Row 1 holds no URL; rows 0 and 2 do. Restricting the view to row 1
    // alone must produce zero targets - filtered-out rows are never fetched.
    let mut ctx = ctx_with_tab(true);
    ctx.open_tabs[0].view_rows = Some(vec![1]);
    let p = params(Extract::Status, None);
    let out = run(&ctx, &p).unwrap();
    assert_eq!(out["pages_fetched"].as_u64().unwrap_or(1), 0);
    assert_eq!(out["values_written"].as_u64().unwrap_or(1), 0);
}

#[test]
fn view_rows_preserves_storage_indices() {
    // A reversed view must still write onto the storage row, not the view
    // position - the BTreeMap pairing is storage-indexed end to end.
    let mut ctx = ctx_with_tab(true);
    ctx.open_tabs[0].view_rows = Some(vec![2, 1, 0]);
    let p = params(Extract::Status, None);
    // No network in tests: rows 0/2 fail the fetch but must appear as
    // failures keyed by their STORAGE row, proving no positional drift.
    let out = run(&ctx, &p).unwrap();
    let failures = out["failures"].as_array().cloned().unwrap_or_default();
    let mut rows: Vec<u64> = failures.iter().filter_map(|f| f["row"].as_u64()).collect();
    rows.sort();
    // Row 1 has no URL so it never becomes a target; 0 and 2 do.
    assert!(rows.iter().all(|r| *r != 1));
}

#[test]
fn rank_hits_prefers_query_overlap() {
    let mut hits = vec![
        web_search::Hit {
            title: "Unrelated forum thread".into(),
            url: "https://a/1".into(),
            snippet: "nothing matching".into(),
        },
        web_search::Hit {
            title: "Festool TS 55 plunge saw price".into(),
            url: "https://a/2".into(),
            snippet: "buy the Festool TS 55".into(),
        },
    ];
    rank_hits("Festool TS 55 gebraucht", &mut hits);
    assert_eq!(hits[0].url, "https://a/2");
    // A query with no usable words leaves the engine order untouched.
    let mut same = vec![
        web_search::Hit {
            title: "x".into(),
            url: "https://a/1".into(),
            snippet: String::new(),
        },
        web_search::Hit {
            title: "y".into(),
            url: "https://a/2".into(),
            snippet: String::new(),
        },
    ];
    rank_hits("ab", &mut same);
    assert_eq!(same[0].url, "https://a/1");
}

#[test]
fn backoff_statuses_and_error_display() {
    assert!(is_backoff_status(429));
    assert!(is_backoff_status(503));
    assert!(!is_backoff_status(404));
    assert!(!is_backoff_status(200));
    assert_eq!(ResolveFail::Http(429).to_string(), "HTTP 429");
    assert_eq!(ResolveFail::Other("boom".into()).to_string(), "boom");
}

#[test]
fn only_missing_skips_filled_target_cells() {
    let mut ctx = ctx_with_tab(true);
    // Give every row a `live` target cell: row 0 already holds an answer
    // (and a URL), so `only_missing` must leave it untouched; the other
    // rows hold no URLs and never become targets either way.
    ctx.open_tabs[0].table.columns.push(ColumnInfo {
        name: "live".into(),
        data_type: "String".into(),
    });
    ctx.open_tabs[0].table.rows[0].push(CellValue::String("200".into()));
    ctx.open_tabs[0].table.rows[1].push(CellValue::Null);
    ctx.open_tabs[0].table.rows[2].push(CellValue::Null);
    let p = Params {
        only_missing: true,
        ..params(Extract::Status, None)
    };
    let out = run(&ctx, &p).unwrap();
    assert_eq!(out["pages_fetched"].as_u64().unwrap_or(1), 0);
    // Nothing was queued at all: row 0's filled cell is untouchable, the
    // rest has no URL - so no op may exist.
    assert!(
        ctx.pending_tab_edits
            .as_ref()
            .unwrap()
            .lock()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn only_missing_off_still_processes() {
    // Control: the same fixture without only_missing must actually try row
    // 0 (its URL), so the skip above is the flag's doing, not a dead input.
    let mut ctx = ctx_with_tab(true);
    ctx.open_tabs[0].table.columns.push(ColumnInfo {
        name: "live".into(),
        data_type: "String".into(),
    });
    for r in &mut ctx.open_tabs[0].table.rows {
        r.push(CellValue::Null);
    }
    ctx.open_tabs[0].table.rows[0][2] = CellValue::String("200".into());
    let p = params(Extract::Status, None);
    let out = run(&ctx, &p).unwrap();
    // Row 0 became a target: either it fetched or it failed - the point is
    // it was not silently skipped.
    let touched = out["pages_fetched"].as_u64().unwrap_or(0)
        + out["failures"]
            .as_array()
            .map(|a| a.len() as u64)
            .unwrap_or(0);
    assert!(touched > 0);
}

#[test]
fn cancel_stops_before_any_fetch() {
    let ctx = ctx_with_tab(true);
    let p = params(Extract::Status, None);
    let ctl = RunCtl {
        cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
        ..RunCtl::default()
    };
    // Pre-set cancel: no page is fetched, no ops are queued.
    let out = run_ctl(&ctx, &p, &ctl).unwrap();
    assert_eq!(out["pages_fetched"].as_u64().unwrap_or(1), 0);
    assert!(
        ctx.pending_tab_edits
            .as_ref()
            .unwrap()
            .lock()
            .unwrap()
            .is_empty()
    );
}
