# DBviewy workflow tests

Run the 50-case suite:

```sh
cargo test --locked --test dbviewy_workflows
```

Run one case by its ID (for example, `cargo test --locked --test
dbviewy_workflows uc15_`). Use `-- --list` to inspect the executable inventory.

| Cases | Area | What the checks protect |
| --- | --- | --- |
| 01–12 | File formats | Read and export real fixtures across CSV, TSV, JSON, JSON Lines, Arrow, Parquet, XLSX, ODS, YAML, gzip, Zstandard and Avro. Check nulls, Unicode, quoted values, sheet selection and exact decimals. |
| 13–24 | Local databases | Reopen SQLite and DuckDB files after real writes. Check row identity, untouched tables, rollback on constraints, schema refusals and query-result export. |
| 25–34 | Formulas and workbooks | Recalculate after edit and Undo, identify a bad reference, and check the difference between a cached Excel value and a preserved formula. |
| 35–44 | SQL | Check the values returned by joins, window functions, paging, grouping and decimal totals. Verify that a rejected query leaves the workspace usable. |
| 45–50 | Automation | Spawn the real CLI. Assert machine-readable output, schema validation and safe writes, including preservation of the last good database after a failed query. |

All fixtures are generated in temporary directories. The suite does not use
server databases, customer files, cloud services or user settings. A passing
case checks the resulting values, an error boundary or persisted data; a
working file picker alone is not a pass. This suite does not replace native
UI checks or gated tests against live database engines.

Excel checks cover DBviewy's supported formula evaluator and the formulas and
cached results read from a workbook. They do not run Excel or promise complete
formula compatibility.
