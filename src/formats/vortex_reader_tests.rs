use super::*;
use crate::data::CellValue;
use crate::formats::FormatRegistry;

#[test]
fn vortex_registry_opens_local_files_read_only() {
    let registry = FormatRegistry::default();
    let reader = registry.reader_for_path(Path::new("test.VORTEX")).unwrap();
    assert_eq!(reader.name(), "Vortex");
    assert!(!reader.supports_write());
    let remote = Path::new("https://example.invalid/data.vortex");
    assert!(
        reader
            .read_file(remote)
            .unwrap_err()
            .to_string()
            .contains("local Vortex file")
    );
}

#[test]
#[ignore = "requires the signed vortex extension installed for the bundled DuckDB"]
fn vortex_native_import_reads_sample_schema_types_and_row_limit() {
    let sample = Path::new(env!("CARGO_MANIFEST_DIR")).join("samples/tables/products.vortex");
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("retail's sample.vortex");
    std::fs::copy(sample, &file).unwrap();
    let registry = FormatRegistry::default();
    let reader = registry.reader_for_path(&file).unwrap();
    let all = reader.read_file(&file).unwrap();
    assert_eq!((all.row_count(), all.col_count()), (8, 8));
    assert_eq!(all.columns, reader.read_schema(&file).unwrap());
    assert_eq!(all.rows[0][0], CellValue::Int(1));
    assert_eq!(all.rows[0][1], CellValue::String("Café Latte".into()));
    assert_eq!(all.rows[0][4], CellValue::Bool(true));
    assert_eq!(all.rows[0][7], CellValue::Null);
    let limited = read(&file, 2).unwrap();
    assert_eq!(limited.total_rows, Some(8));
    assert_eq!(limited.rows, all.rows[..2]);
}
