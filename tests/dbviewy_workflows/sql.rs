use super::*;
use octa::sql::{QueryKind, SqlWorkspace, TableOrigin, run_query};

#[test]
fn uc35_monthly_regional_report_groups_the_correct_sales() {
    let fx = Fixture::new();
    let sales = read(&fx.file(
        "sales.csv",
        "region,month,qty\nEast,2026-11,7\nWest,2026-11,11\nEast,2026-10,50\nEast,2026-11,4\n",
    ));
    let result = run_query(&sales,
        "SELECT region, sum(qty) AS total FROM data WHERE month = '2026-11' GROUP BY region ORDER BY region",
    ).unwrap();
    assert_eq!(result.kind, QueryKind::Select);
    text(&result.table, "region", &["East", "West"]);
    // SUM over integer rows is HUGEINT, exposed as exact text, not rounded f64.
    text(&result.table, "total", &["11", "11"]);
}

#[test]
fn uc36_top_supplier_per_region_resets_the_window_ranking() {
    let fx = Fixture::new();
    let input = read(&fx.file(
        "suppliers.csv",
        "region,sku,volume\nEast,E-2,4\nWest,W-1,7\nEast,E-1,9\nWest,W-2,3\n",
    ));
    let result = run_query(&input,
        "SELECT region,sku,volume FROM (
            SELECT *, row_number() OVER(PARTITION BY region ORDER BY volume DESC) AS position FROM data
         ) best WHERE position=1 ORDER BY region",
    ).unwrap().table;
    text(&result, "sku", &["E-1", "W-1"]);
    numbers(&result, "volume", &[9.0, 7.0]);
}

#[test]
fn uc37_exporting_a_paged_query_gets_all_rows_not_just_the_visible_page() {
    let fx = Fixture::new();
    let input = read(&fx.file("products.csv", "id,sku\n3,C\n1,A\n4,D\n2,B\n"));
    let mut ws = SqlWorkspace::new().unwrap();
    ws.set_active_table(&input).unwrap();
    let page = ws
        .execute_paged("SELECT id,sku FROM data ORDER BY id", 2, 17)
        .unwrap();
    assert_eq!(page.total_rows, Some(4));
    numbers(&page.outcome.table, "id", &[1.0, 2.0]);
    numbers(&ws.result_page(17, 2, 2).unwrap(), "id", &[3.0, 4.0]);
    let out = fx.path("all.csv");
    write(&out, &ws.result_all(17).unwrap());
    text(&read(&out), "sku", &["A", "B", "C", "D"]);
}

#[test]
fn uc38_left_join_csv_orders_with_json_customers_keeps_unmatched_orders() {
    let fx = Fixture::new();
    let input = read(&fx.file("orders.csv", "id,total\n1,20\n2,45\n3,17\n"));
    let dim = fx.file(
        "customers.json",
        r#"[{"id":1,"customer":"Ada"},{"id":3,"customer":"Jules"}]"#,
    );
    let mut ws = SqlWorkspace::new().unwrap();
    ws.set_active_table(&input).unwrap();
    ws.add_table_from_file(&dim, None, "customers").unwrap();
    let grid = ws.execute(
        "SELECT a.id,c.customer,a.total FROM data a LEFT JOIN customers c USING(id) ORDER BY a.id",
    ).unwrap().table;
    numbers(&grid, "total", &[20.0, 45.0, 17.0]);
    assert_eq!(
        cells(&grid, "customer"),
        [
            CellValue::String("Ada".into()),
            CellValue::Null,
            CellValue::String("Jules".into())
        ]
    );
}

#[test]
fn uc39_revenue_report_accepts_quoted_column_names_verbatim() {
    let fx = Fixture::new();
    let input = read(&fx.file(
        "office.csv",
        "Account name,Net amount (€)\nA,7\nB,12\nA,5\n",
    ));
    let grid = run_query(
        &input,
        r#"SELECT "Account name" AS account, sum("Net amount (€)") AS net FROM data GROUP BY 1 ORDER BY 1"#,
    ).unwrap().table;
    text(&grid, "account", &["A", "B"]);
    text(&grid, "net", &["12", "12"]);
}

#[test]
fn uc40_missing_scores_are_reported_separately_from_explicit_zero() {
    let fx = Fixture::new();
    let input = read(&fx.file("surveys.tsv", "team\tscore\nOps\t0\nOps\t\nOps\t12\n"));
    let grid = run_query(
        &input,
        "SELECT count(*) AS responses,count(score) AS scored,
         count(*) FILTER (WHERE score=0) AS zeroes,avg(score) AS average FROM data",
    )
    .unwrap()
    .table;
    numbers(&grid, "responses", &[3.0]);
    numbers(&grid, "scored", &[2.0]);
    numbers(&grid, "zeroes", &[1.0]);
    numbers(&grid, "average", &[6.0]);
}

#[test]
fn uc41_sql_edit_of_a_scratch_table_is_not_an_edit_to_the_source() {
    let original = table(
        &[("id", "Int64"), ("qty", "Int64")],
        vec![
            vec![CellValue::Int(1), CellValue::Int(10)],
            vec![CellValue::Int(2), CellValue::Int(20)],
        ],
    );
    let mut ws = SqlWorkspace::new().unwrap();
    ws.set_active_table(&original).unwrap();
    let updated = ws.execute("UPDATE data SET qty=25 WHERE id=2").unwrap();
    assert_eq!(updated.kind, QueryKind::Mutation);
    let after = ws
        .execute("SELECT id,qty FROM data ORDER BY id")
        .unwrap()
        .table;
    numbers(&after, "qty", &[10.0, 25.0]);
    numbers(&original, "qty", &[10.0, 20.0]);
}

#[test]
fn uc42_union_by_name_preserves_columns_across_two_format_exports() {
    let fx = Fixture::new();
    let src = read(&fx.file("old.csv", "id,city\n1,Berlin\n"));
    let add = read(&fx.file("new.tsv", "city\tid\tregion\nParis\t2\tEU\n"));
    let path = fx.path("new.parquet");
    write(&path, &add);
    let mut ws = SqlWorkspace::new().unwrap();
    ws.set_active_table(&src).unwrap();
    ws.add_table("incoming", &read(&path), TableOrigin::ActiveTab)
        .unwrap();
    let result = ws
        .execute("SELECT * FROM data UNION ALL BY NAME SELECT * FROM incoming ORDER BY id")
        .unwrap()
        .table;
    numbers(&result, "id", &[1.0, 2.0]);
    text(&result, "city", &["Berlin", "Paris"]);
    assert_eq!(
        cells(&result, "region"),
        [CellValue::Null, CellValue::String("EU".into())]
    );
}

#[test]
fn uc43_bad_query_does_not_destroy_the_open_workspace() {
    let fx = Fixture::new();
    let input = read(&fx.file("stock.csv", "sku,qty\nA,1\nB,4\n"));
    let mut ws = SqlWorkspace::new().unwrap();
    ws.set_active_table(&input).unwrap();
    let error = ws
        .execute("UPDATE data SET does_not_exist=30")
        .err()
        .expect("bad column rejected");
    assert!(format!("{error:#}").contains("does_not_exist"), "{error:#}");
    let after = ws
        .execute("SELECT sku,qty FROM data ORDER BY sku")
        .unwrap()
        .table;
    text(&after, "sku", &["A", "B"]);
    numbers(&after, "qty", &[1.0, 4.0]);
}

#[test]
fn uc44_month_end_decimal_totals_do_not_gain_float_noise() {
    let fx = Fixture::new();
    let input = read(&fx.file(
        "ledger.csv",
        "account,amount\nA,0.1\nA,0.2\nA,0.3\nB,0.2\nB,0.7\n",
    ));
    let result = run_query(
        &input,
        "SELECT account,sum(amount) AS total FROM data GROUP BY account ORDER BY account",
    )
    .unwrap()
    .table;
    text(&result, "account", &["A", "B"]);
    let values: Vec<_> = cells(&result, "total")
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(values, ["0.6", "0.9"]);
}
