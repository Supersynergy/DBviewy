//! Local Vortex files, using DuckDB's installed, signed core extension.
use super::parquet_reader::{arrow_type_name, arrow_value_to_cell};
use super::{FormatReader, initial_load_rows};
use crate::data::{ColumnInfo, DataTable};
use anyhow::{Context, Result, ensure};
use duckdb::{Config, Connection};
use std::path::Path;

pub struct VortexReader;

impl FormatReader for VortexReader {
    fn name(&self) -> &str {
        "Vortex"
    }
    fn extensions(&self) -> &[&str] {
        &["vortex"]
    }
    fn read_file(&self, path: &Path) -> Result<DataTable> {
        read(path, initial_load_rows())
    }
    fn read_schema(&self, path: &Path) -> Result<Vec<ColumnInfo>> {
        Ok(read(path, 0)?.columns)
    }
}

fn read(path: &Path, row_limit: usize) -> Result<DataTable> {
    let local = path.canonicalize().context("Resolving local Vortex file")?;
    ensure!(local.is_file(), "Choose a local Vortex file");
    // The extension's local object-store adapter percent-encodes spaces and
    // non-ASCII names after DuckDB resolves globs. Give it an unambiguous
    // local alias for these paths, keeping the original path on the table.
    let mut alias_dir = None;
    let alias;
    let scan_path = if local.to_str().is_some_and(|s| {
        s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/._-:\\".contains(&b))
    }) {
        local.as_path()
    } else {
        #[cfg(unix)]
        let dir = tempfile::tempdir_in("/tmp")?;
        #[cfg(not(unix))]
        let dir = tempfile::tempdir()?;
        alias = dir.path().join("data.vortex");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&local, &alias)?;
        #[cfg(not(unix))]
        if std::fs::hard_link(&local, &alias).is_err() {
            std::fs::copy(&local, &alias)?;
        }
        alias_dir = Some(dir);
        alias.as_path()
    };
    let _keep_alias_until_query_finishes = alias_dir;
    let filename = scan_path
        .to_str()
        .context("Vortex needs a UTF-8 scan path")?;
    let config = Config::default()
        .with("autoinstall_known_extensions", "false")?
        .with("autoload_known_extensions", "false")?
        .with("allow_unsigned_extensions", "false")?
        .with("allow_community_extensions", "false")?;
    let conn = Connection::open_in_memory_with_flags(config)?;
    conn.execute_batch("LOAD vortex").context(
        "Vortex needs the signed core extension for Octa's bundled DuckDB version. Install it with INSTALL vortex in a matching DuckDB, then reopen the file",
    )?;
    // DuckDB's SET syntax takes a literal list. Limit external access to
    // the chosen local file; the scan itself binds its path as a parameter.
    let escaped = filename.replace('\'', "''");
    conn.execute_batch(&format!(
        "SET allowed_paths=['{escaped}']; SET enable_external_access=false"
    ))?;
    let sql = match row_limit {
        0 => "SELECT * FROM read_vortex(?) LIMIT 0".to_owned(),
        usize::MAX => "SELECT * FROM read_vortex(?)".to_owned(),
        n => format!("SELECT * FROM read_vortex(?) LIMIT {}", n + 1),
    };
    let mut statement = conn.prepare(&sql)?;
    let batches = statement.query_arrow([filename])?;
    let columns = batches
        .get_schema()
        .fields()
        .iter()
        .map(|f| ColumnInfo {
            name: f.name().clone(),
            data_type: arrow_type_name(f.data_type()),
        })
        .collect();
    let mut rows = Vec::new();
    let mut truncated = false;
    for batch in batches {
        for r in 0..batch.num_rows() {
            if rows.len() == row_limit {
                truncated = true;
                break;
            }
            rows.push(
                batch
                    .columns()
                    .iter()
                    .map(|col| arrow_value_to_cell(col.as_ref(), r))
                    .collect(),
            );
        }
    }
    let total_rows = if truncated {
        Some(
            conn.query_row("SELECT count(*) FROM read_vortex(?)", [filename], |r| {
                r.get::<_, usize>(0)
            })?,
        )
    } else {
        None
    };
    let mut table = DataTable {
        columns,
        rows,
        total_rows,
        source_path: Some(path.to_string_lossy().into_owned()),
        format_name: Some("Vortex".into()),
        ..DataTable::empty()
    };
    crate::data::times::settle_interval_columns(&mut table);
    Ok(table)
}

#[cfg(test)]
#[path = "vortex_reader_tests.rs"]
mod tests;
