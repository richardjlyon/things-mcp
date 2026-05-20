//! Run-once schema probe asserting that every column our queries reference
//! exists on disk. Lets us fail fast with a clear message rather than return
//! garbage if a future Things upgrade renames or removes a column.

use std::path::Path;

use rusqlite::{Connection, OpenFlags};

use crate::core::error::ThingsError;

/// (table, column) pairs the read path depends on. Add to this list as new
/// queries land.
const REQUIRED: &[(&str, &[&str])] = &[
    (
        "TMTask",
        &[
            "uuid",
            "title",
            "type",
            "status",
            "trashed",
            "start",
            "project",
            "area",
            "heading",
            "notes",
            "creationDate",
            "userModificationDate",
            "startDate",
            "deadline",
            "stopDate",
            "rt1_recurrenceRule",
            "todayIndex",
            "index",
        ],
    ),
    ("TMArea", &["uuid", "title", "index"]),
    ("TMTag", &["uuid", "title", "shortcut", "parent", "index"]),
    ("TMTaskTag", &["tasks", "tags"]),
    (
        "TMChecklistItem",
        &["uuid", "title", "status", "task", "index"],
    ),
];

pub fn probe(db_path: &Path) -> Result<(), ThingsError> {
    let c = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let mut missing = Vec::new();
    for (table, cols) in REQUIRED {
        let table_cols = list_columns(&c, table)?;
        for col in *cols {
            if !table_cols.iter().any(|t| t.eq_ignore_ascii_case(col)) {
                missing.push(format!("{table}.{col}"));
            }
        }
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(ThingsError::SchemaIncompatible {
            missing,
            things_version_guess: None,
        })
    }
}

fn list_columns(c: &Connection, table: &str) -> Result<Vec<String>, rusqlite::Error> {
    let mut stmt = c.prepare(&format!("PRAGMA table_info(\"{}\")", table))?;
    let cols: Result<Vec<String>, _> = stmt.query_map([], |r| r.get::<_, String>(1))?.collect();
    cols
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reader::fixture::build_fixture;
    use tempfile::tempdir;

    #[test]
    fn probe_passes_on_fixture() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("ok.sqlite");
        build_fixture(&path).unwrap();
        probe(&path).expect("schema probe should pass on fixture");
    }

    #[test]
    fn probe_reports_missing_columns() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("bad.sqlite");
        let c = Connection::open(&path).unwrap();
        // intentionally drop most of the columns
        c.execute_batch("CREATE TABLE TMTask (uuid TEXT);").unwrap();
        let err = probe(&path).unwrap_err();
        match err {
            ThingsError::SchemaIncompatible { missing, .. } => {
                assert!(missing.iter().any(|m| m == "TMTask.title"));
                assert!(missing.iter().any(|m| m == "TMTask.status"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
