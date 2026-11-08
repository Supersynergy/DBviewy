//! Recently-opened things that are not files.
//!
//! `recent.toml` already tracks the File -> Recent files list; the same
//! file carries this list too, so a database table the user opened last
//! week is one menu click away instead of a walk through the connection
//! tree. Lives in the library because the toolbar menus (library code)
//! render the entries.

/// One database object the user opened, for the File -> Recent databases
/// submenu. Two entries are "the same" on everything but `conn_name`,
/// which is a display snapshot - a renamed connection keeps opening fine.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RecentDbEntry {
    /// The saved connection's id (not its name - ids survive renames).
    pub conn_id: String,
    /// Display snapshot for the menu label: `table @ conn_name`.
    pub conn_name: String,
    /// Catalog, for engines that have the extra level (Databricks et al.).
    #[serde(default)]
    pub catalog: Option<String>,
    pub schema: String,
    pub table: String,
}

impl RecentDbEntry {
    /// The menu label: `catalog.schema.table @ conn`, minus empty parts.
    pub fn label(&self) -> String {
        let mut parts: Vec<&str> = Vec::new();
        if let Some(c) = &self.catalog {
            if !c.is_empty() {
                parts.push(c);
            }
        }
        if !self.schema.is_empty() {
            parts.push(&self.schema);
        }
        parts.push(&self.table);
        format!("{} @ {}", parts.join("."), self.conn_name)
    }

    /// Identity for dedupe: same object on the same connection, regardless
    /// of what the connection was renamed to since.
    pub fn same_object(&self, other: &Self) -> bool {
        self.conn_id == other.conn_id
            && self.catalog == other.catalog
            && self.schema == other.schema
            && self.table == other.table
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> RecentDbEntry {
        RecentDbEntry {
            conn_id: "c1".into(),
            conn_name: "prod".into(),
            catalog: Some("main".into()),
            schema: "sales".into(),
            table: "lots".into(),
        }
    }

    #[test]
    fn label_qualifies_and_names() {
        assert_eq!(entry().label(), "main.sales.lots @ prod");
        let mut e = entry();
        e.catalog = None;
        e.schema = String::new();
        assert_eq!(e.label(), "lots @ prod");
    }

    #[test]
    fn same_object_ignores_conn_name_drift() {
        let mut other = entry();
        other.conn_name = "renamed".into();
        assert!(entry().same_object(&other));
        other.schema = "other".into();
        assert!(!entry().same_object(&other));
    }
}
