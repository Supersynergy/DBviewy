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
        cached: false,
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
