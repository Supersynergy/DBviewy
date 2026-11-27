//! Unit tests for [`chart_export`](chart_export). Split out of the source file; included
//! back via `#[path]` so it stays an inner `tests` module with access to the
//! parent module's private items.

use super::*;
use crate::data::chart::{BoxSummary, ChartPrep, ChartSeries, XAxisKind};

fn prep_lines() -> ChartPrep {
    ChartPrep {
        data: ChartData::Lines {
            categories: None,
            series: vec![ChartSeries {
                name: "y".into(),
                points: vec![[0.0, 1.0], [1.0, 2.0], [2.0, 1.5]],
            }],
        },
        total_rows: 3,
        used_rows: 3,
        x_label: "x".into(),
        y_label: "y".into(),
        x_axis_kind: XAxisKind::Numeric,
        y_axis_kind: XAxisKind::Numeric,
    }
}

#[test]
fn svg_emits_well_formed_root() {
    let svg = to_svg(
        &prep_lines(),
        &ExportOptions {
            title: "T".into(),
            x_label: "X".into(),
            y_label: "Y".into(),
            legend: LegendPosition::Off,
            series: vec![ResolvedSeries {
                display_name: "y".into(),
                color: None,
            }],
            pareto: false,
        },
    );
    assert!(svg.starts_with("<svg"));
    assert!(svg.contains("</svg>"));
    assert!(svg.contains(">T<"));
}

#[test]
fn svg_includes_legend_when_position_set() {
    let svg = to_svg(
        &prep_lines(),
        &ExportOptions {
            title: String::new(),
            x_label: "X".into(),
            y_label: "Y".into(),
            legend: LegendPosition::TopRight,
            series: vec![ResolvedSeries {
                display_name: "my-legend".into(),
                color: None,
            }],
            pareto: false,
        },
    );
    assert!(svg.contains(">my-legend<"), "{svg}");
}

#[test]
fn svg_escapes_special_chars() {
    let svg = to_svg(
        &prep_lines(),
        &ExportOptions {
            title: "a & b < c".into(),
            x_label: "X".into(),
            y_label: "Y".into(),
            legend: LegendPosition::Off,
            series: Vec::new(),
            pareto: false,
        },
    );
    assert!(svg.contains("a &amp; b &lt; c"));
}

#[test]
fn png_decodes_to_non_empty_buffer() {
    let svg = to_svg(
        &prep_lines(),
        &ExportOptions {
            title: "T".into(),
            x_label: "X".into(),
            y_label: "Y".into(),
            legend: LegendPosition::Off,
            series: Vec::new(),
            pareto: false,
        },
    );
    let png = to_png(&svg, 1.0).unwrap();
    // PNG signature: 89 50 4E 47 0D 0A 1A 0A
    assert_eq!(&png[..4], b"\x89PNG");
}

#[test]
fn pdf_starts_with_magic() {
    let svg = to_svg(
        &prep_lines(),
        &ExportOptions {
            title: "T".into(),
            x_label: "X".into(),
            y_label: "Y".into(),
            legend: LegendPosition::Off,
            series: Vec::new(),
            pareto: false,
        },
    );
    let pdf = to_pdf(&svg).unwrap();
    assert_eq!(&pdf[..4], b"%PDF");
}

#[test]
fn box_summary_serialises() {
    let prep = ChartPrep {
        data: ChartData::Boxes(vec![BoxSummary {
            name: "v".into(),
            lower_whisker: 1.0,
            q1: 2.0,
            median: 3.0,
            q3: 4.0,
            upper_whisker: 5.0,
        }]),
        total_rows: 5,
        used_rows: 5,
        x_label: "Series".into(),
        y_label: "Value".into(),
        x_axis_kind: XAxisKind::Numeric,
        y_axis_kind: XAxisKind::Numeric,
    };
    let svg = to_svg(
        &prep,
        &ExportOptions {
            title: String::new(),
            x_label: "Series".into(),
            y_label: "Value".into(),
            legend: LegendPosition::Off,
            series: vec![ResolvedSeries {
                display_name: "v".into(),
                color: None,
            }],
            pareto: false,
        },
    );
    assert!(svg.contains("<rect"));
    assert!(svg.contains(">v<"));
}

fn blank_opts() -> ExportOptions {
    ExportOptions {
        title: String::new(),
        x_label: String::new(),
        y_label: String::new(),
        legend: LegendPosition::Off,
        series: Vec::new(),
        pareto: false,
    }
}

#[test]
fn a_heatmap_exports_one_rect_per_filled_cell_and_renders() {
    use crate::data::CellValue;
    let prep = crate::data::chart::heatmap_prep(
        vec![
            crate::data::chart::HeatGroup {
                x: CellValue::String("a".into()),
                y: CellValue::String("p".into()),
                value: Some(1.0),
                rows: 1,
            },
            crate::data::chart::HeatGroup {
                x: CellValue::String("b".into()),
                y: CellValue::String("p".into()),
                value: Some(3.0),
                rows: 2,
            },
            crate::data::chart::HeatGroup {
                x: CellValue::String("b".into()),
                y: CellValue::String("q".into()),
                value: Some(-2.0),
                rows: 1,
            },
        ],
        "x",
        "y",
        4,
        200,
    )
    .unwrap();
    let svg = to_svg(&prep, &blank_opts());
    assert_eq!(svg.matches(r#"class="cell""#).count(), 3);
    assert!(to_png(&svg, 1.0).is_ok());
}

#[test]
fn a_calendar_exports_one_rect_per_day_across_years() {
    let prep = crate::data::chart::calendar_prep(
        vec![(19_722, 1.0), (19_723, 2.0), (20_088, 5.0)],
        "d",
        "v",
        3,
    )
    .unwrap();
    let svg = to_svg(&prep, &blank_opts());
    assert_eq!(svg.matches(r#"class="cell""#).count(), 3);
    assert!(svg.contains(">2023<") && svg.contains(">2024<"));
    assert!(to_png(&svg, 1.0).is_ok());
}

#[test]
fn a_pareto_export_draws_the_share_line_and_its_percent_scale() {
    let prep = ChartPrep {
        data: ChartData::Bars {
            categories: vec!["a".into(), "b".into()],
            series: vec![ChartSeries {
                name: "revenue".into(),
                points: vec![[0.0, 75.0], [1.0, 25.0]],
            }],
        },
        ..prep_lines()
    };
    let plain = to_svg(&prep, &blank_opts());
    assert!(!plain.contains("<polyline"), "a bar chart has no line");
    let svg = to_svg(&prep, &blank_opts().with_pareto(true));
    assert!(svg.contains("<polyline") && svg.contains(">100%<"), "{svg}");
}

fn attribute_number(line: &str, key: &str) -> f64 {
    line.split_once(&format!(" {key}=\""))
        .unwrap()
        .1
        .split_once('"')
        .unwrap()
        .0
        .parse()
        .unwrap()
}

fn categorical_chart(scatter: bool, categories: &[&str], points: Vec<[f64; 2]>) -> ChartPrep {
    let series = vec![ChartSeries {
        name: "Measured".into(),
        points,
    }];
    let categories = Some(categories.iter().map(|s| (*s).to_string()).collect());
    let data = if scatter {
        ChartData::Scatter { series, categories }
    } else {
        ChartData::Lines { series, categories }
    };
    ChartPrep {
        data,
        ..prep_lines()
    }
}

#[test]
fn exported_categorical_line_points_share_x_coordinates_with_their_labels() {
    let prep = categorical_chart(
        false,
        &[
            "Jan 2026", "Feb 2026", "Mar 2026", "Apr 2026", "May 2026", "Jun 2026",
        ],
        vec![
            [0.0, 100.0],
            [1.0, 115.0],
            [2.0, 120.5],
            [3.0, 130.0],
            [4.0, 109.0],
            [5.0, 140.0],
        ],
    );
    let svg = to_svg(&prep, &blank_opts());
    let actual: Vec<_> = svg
        .lines()
        .filter(|line| line.contains("<circle cx="))
        .map(|line| attribute_number(line, "cx"))
        .collect();
    let labels: Vec<_> = svg
        .lines()
        .filter(|line| line.contains(" 2026</text>"))
        .map(|line| attribute_number(line, "x"))
        .collect();
    assert_eq!(actual.len(), 6);
    assert_eq!(actual, labels);
    assert!(actual[0] > PAD_LEFT && actual[5] < SVG_W - PAD_RIGHT);
}

#[test]
fn exported_scatter_preserves_empty_categorical_slots() {
    let prep = categorical_chart(
        true,
        &["Empty first", "Pair A", "Gap", "Pair B", "Empty last"],
        vec![[1.0, 10.0], [3.0, 20.0]],
    );
    let svg = to_svg(&prep, &blank_opts());
    let label_x = |label: &str| {
        let needle = format!(">{label}</text>");
        attribute_number(
            svg.lines().find(|line| line.contains(&needle)).unwrap(),
            "x",
        )
    };
    let actual: Vec<_> = svg
        .lines()
        .filter(|line| line.contains("<circle cx="))
        .map(|line| attribute_number(line, "cx"))
        .collect();
    assert_eq!(actual, [label_x("Pair A"), label_x("Pair B")]);
    assert!(label_x("Empty first") < actual[0] && actual[1] < label_x("Empty last"));
    assert_eq!(label_x("Gap"), (actual[0] + actual[1]) / 2.0);
    assert!(to_png(&svg, 0.5).unwrap().starts_with(b"\x89PNG\r\n\x1a\n"));
}

#[test]
fn datetime_tick_endpoints_anchor_text_inward_without_moving_values() {
    let mut svg = String::new();
    // A span long enough to include literal times on the first and last tick.
    emit_x_axis_ticks(
        &mut svg,
        (1_767_225_600.0, 1_780_272_000.0),
        XAxisKind::DateTime,
    );
    let ticks: Vec<_> = svg.lines().collect();
    assert_eq!(ticks.len(), 6);
    assert_eq!(attribute_number(ticks[0], "x"), PAD_LEFT);
    assert_eq!(attribute_number(ticks[5], "x"), SVG_W - PAD_RIGHT);
    assert!(ticks[0].contains("text-anchor=\"start\"") && ticks[5].contains("text-anchor=\"end\""));
    assert!(
        ticks[1..5]
            .iter()
            .all(|tick| tick.contains("text-anchor=\"middle\""))
    );
    assert!(ticks[5].contains("00:00:00"), "{}", ticks[5]);
}
