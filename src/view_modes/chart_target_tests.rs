use super::*;
use octa::data::chart::{LegendPosition, XAxisKind};

fn chart(points: Vec<[f64; 2]>, categories: Option<Vec<String>>) -> ChartPrep {
    ChartPrep {
        total_rows: points.len(),
        used_rows: points.len(),
        data: ChartData::Lines {
            categories,
            series: vec![ChartSeries {
                name: "Revenue".into(),
                points,
            }],
        },
        x_label: "Month".into(),
        y_label: "Sales".into(),
        x_axis_kind: XAxisKind::Numeric,
        y_axis_kind: XAxisKind::Numeric,
    }
}

fn line_cfg() -> ChartConfig {
    ChartConfig {
        kind: ChartKind::Line,
        y_cols: vec![1],
        ..ChartConfig::default()
    }
}

fn lines(prep: &ChartPrep) -> &[ChartSeries] {
    let ChartData::Lines { series, .. } = &prep.data else {
        panic!("expected lines");
    };
    series
}

#[test]
fn target_line_keeps_the_raw_scale_and_the_first_series_in_place() {
    let points = vec![[1.0, 3.0], [10.0, 5.0], [31.0, 7.0]];
    let mut prep = chart(points.clone(), None);
    prep.x_axis_kind = XAxisKind::Date;
    let target = append(&mut prep, &line_cfg(), " 4,5 ").unwrap();
    assert_eq!(target, 1);
    assert_eq!(lines(&prep)[0].points, points);
    assert_eq!(lines(&prep)[target].points, [[1.0, 4.5], [31.0, 4.5]]);
    assert_eq!(
        lines(&prep)[target].name,
        format!("{}: 4,5", t("chart.target_line"))
    );
    assert_eq!(prep.total_rows, 3);
    assert_eq!(prep.used_rows, 3);
}

#[test]
fn target_line_obeys_fixed_y_bounds_and_handles_non_numeric_x_labels() {
    let cfg = ChartConfig {
        y_min: Some(0.0),
        y_max: Some(20.0),
        ..line_cfg()
    };
    let mut prep = chart(
        vec![[0.0, 10.0], [2.0, 15.0], [0.0, 11.0]],
        Some(vec!["A".into(), "B".into(), "C".into()]),
    );
    for input in ["", "NaN", "-inf", "1.2,5", "-1", "20.1"] {
        assert!(append(&mut prep, &cfg, input).is_none(), "{input}");
    }
    let log = ChartConfig {
        y_log_scale: true,
        ..cfg.clone()
    };
    assert!(append(&mut prep, &log, "10").is_none());
    let scatter = ChartConfig {
        kind: ChartKind::Scatter,
        ..cfg.clone()
    };
    assert!(append(&mut prep, &scatter, "10").is_none());
    assert_eq!(lines(&prep).len(), 1);
    assert_eq!(append(&mut prep, &cfg, "0"), Some(1));
    assert_eq!(lines(&prep)[1].points, [[0.0, 0.0], [2.0, 0.0]]);

    let mut single = chart(vec![[5.0, 10.0]], None);
    append(&mut single, &cfg, "20").unwrap();
    assert_eq!(lines(&single)[1].points, [[4.0, 20.0], [6.0, 20.0]]);
    assert!(append(&mut chart(vec![], None), &cfg, "10").is_none());
}

#[test]
fn target_line_exports_as_a_coloured_horizontal_line_in_all_formats() {
    use super::super::{chart_export, export_options};

    let mut prep = chart(vec![[1.0, 10.0], [2.0, 20.0]], None);
    let mut cfg = line_cfg();
    cfg.legend = LegendPosition::TopRight;
    cfg.series_styles.insert(
        1,
        SeriesStyle {
            display_name: "Measured".into(),
            color: Some([20, 60, 140, 255]),
        },
    );
    let slot = append(&mut prep, &cfg, "15").unwrap();
    let opts = export_options(&prep, &cfg, Some(slot));
    assert_eq!(opts.series[0].display_name, "Measured");
    assert_eq!(opts.series[0].color, Some([20, 60, 140, 255]));
    assert_eq!(opts.series[slot].color, Some(AMBER));
    let svg = chart_export::to_svg(&prep, &opts);
    assert!(svg.contains(&format!("{}: 15", t("chart.target_line"))));
    let path = svg
        .lines()
        .find(|line| line.contains("<path") && line.contains("stroke=\"rgba(174,107,25,1.000)\""))
        .expect("target path in exported SVG");
    let coords: Vec<_> = path.split('"').nth(1).unwrap().split_whitespace().collect();
    assert_eq!(coords.len(), 2, "{path}");
    let end = |part: &str| {
        let (_, y) = part.split_once(',').unwrap();
        y.parse::<f64>().unwrap()
    };
    assert_eq!(end(coords[0]), end(coords[1]));
    assert_ne!(
        coords[0][1..].split(',').next(),
        coords[1][1..].split(',').next()
    );
    assert!(
        chart_export::to_png(&svg, 0.5)
            .unwrap()
            .starts_with(b"\x89PNG\r\n\x1a\n")
    );
    assert!(chart_export::to_pdf(&svg).unwrap().starts_with(b"%PDF-"));
}
