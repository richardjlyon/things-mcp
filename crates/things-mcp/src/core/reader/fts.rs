//! FTS5 capability detection.
//!
//! At startup we inspect `sqlite_master` for any virtual table created
//! `USING fts5`, surface the resolved table name + columns on `AppState`,
//! and log the result. The search query path does not yet consume this —
//! activation waits until we can verify Things' actual FTS5 schema against
//! a live install. Until then, this module is purely informational.

use rusqlite::Connection;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FtsCapability {
    /// Discovered virtual table name (e.g. `TMTask_searchstr_data`).
    pub table: String,
    /// Columns the FTS5 index exposes (from `PRAGMA table_info`).
    pub columns: Vec<String>,
}

/// Inspect `sqlite_master` for an FTS5 virtual table that looks Things-related.
/// Returns the first match (table + its columns) or `None` if no FTS5 indices
/// are present.
///
/// Heuristic: any `CREATE VIRTUAL TABLE ... USING fts5(...)` whose name starts
/// with `TMTask` or `TMSearchInfo`. This may be over- or under-inclusive on
/// future Things versions; activation in the search query is gated by an
/// explicit verification step in a later plan.
pub fn detect(conn: &Connection) -> rusqlite::Result<Option<FtsCapability>> {
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master \
         WHERE type = 'table' \
           AND sql IS NOT NULL \
           AND sql LIKE '%USING fts5%' \
           AND (name LIKE 'TMTask%' OR name LIKE 'TMSearchInfo%') \
         ORDER BY name \
         LIMIT 1",
    )?;
    let mut rows = stmt.query([])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    let table: String = row.get(0)?;
    let columns = list_columns(conn, &table)?;
    Ok(Some(FtsCapability { table, columns }))
}

fn list_columns(conn: &Connection, table: &str) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info(\"{}\")", table))?;
    let cols: Result<Vec<String>, _> = stmt
        .query_map([], |r| r.get::<_, String>(1))?
        .collect();
    cols
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn detect_returns_none_on_empty_db() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE TMTask (uuid TEXT);").unwrap();
        assert_eq!(detect(&c).unwrap(), None);
    }

    #[test]
    fn detect_finds_fts5_virtual_table() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE TMTask (uuid TEXT);
             CREATE VIRTUAL TABLE TMTask_searchstr USING fts5(title, notes, content='');",
        )
        .unwrap();
        let cap = detect(&c).unwrap().expect("FTS5 table should be detected");
        assert_eq!(cap.table, "TMTask_searchstr");
        assert!(cap.columns.iter().any(|c| c == "title"));
        assert!(cap.columns.iter().any(|c| c == "notes"));
    }

    #[test]
    fn detect_ignores_non_fts_virtual_tables() {
        let c = Connection::open_in_memory().unwrap();
        // r-tree is another virtual table type; should not be misidentified.
        c.execute_batch(
            "CREATE TABLE TMTask (uuid TEXT);
             CREATE VIRTUAL TABLE TMTask_rtree USING rtree(id, minX, maxX);",
        )
        .unwrap();
        assert_eq!(detect(&c).unwrap(), None);
    }

    #[test]
    fn detect_ignores_unrelated_fts_tables() {
        let c = Connection::open_in_memory().unwrap();
        // An FTS5 table that doesn't look TMTask-related must not match.
        c.execute_batch(
            "CREATE TABLE TMTask (uuid TEXT);
             CREATE VIRTUAL TABLE Whatever_fts USING fts5(stuff);",
        )
        .unwrap();
        assert_eq!(detect(&c).unwrap(), None);
    }
}
