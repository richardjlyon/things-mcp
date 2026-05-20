//! In-code builder for a minimal Things-shaped SQLite, used by tests.
//!
//! Mirrors only the columns we currently query. The real Things schema has
//! many more columns; the reader code never selects `*`, so omissions here
//! don't matter as long as the columns our queries reference are present.

use std::path::Path;

use rusqlite::Connection;

pub fn build_fixture(path: &Path) -> anyhow::Result<()> {
    let c = Connection::open(path)?;
    c.execute_batch(r#"
        CREATE TABLE TMTask (
            uuid TEXT PRIMARY KEY,
            title TEXT,
            type INTEGER,
            status INTEGER,
            trashed INTEGER,
            start INTEGER,
            startDate INTEGER,
            deadline INTEGER,
            stopDate REAL,
            creationDate REAL,
            userModificationDate REAL,
            project TEXT,
            area TEXT,
            heading TEXT,
            notes TEXT,
            rt1_recurrenceRule BLOB,
            "index" INTEGER,
            todayIndex INTEGER
        );
        CREATE TABLE TMArea (uuid TEXT PRIMARY KEY, title TEXT, "index" INTEGER);
        CREATE TABLE TMTag (uuid TEXT PRIMARY KEY, title TEXT, "index" INTEGER, shortcut TEXT, parent TEXT);
        CREATE TABLE TMTaskTag (tasks TEXT, tags TEXT);
        CREATE TABLE Meta (key TEXT PRIMARY KEY, value TEXT);

        INSERT INTO Meta (key, value) VALUES ('databaseVersion', '21');

        INSERT INTO TMArea (uuid, title, "index") VALUES
            ('area-1', 'Personal', 0);

        INSERT INTO TMTag (uuid, title, "index", shortcut, parent) VALUES
            ('tag-errand', 'Errand', 0, NULL, NULL);

        -- Three inbox to-dos (start=0), one completed, two open.
        INSERT INTO TMTask
            (uuid, title, type, status, trashed, start, creationDate, userModificationDate)
        VALUES
            ('todo-1', 'Buy milk',          0, 0, 0, 0, 1715000000.0, 1715000100.0),
            ('todo-2', 'Call the dentist',  0, 0, 0, 0, 1715000200.0, 1715000300.0),
            ('todo-3', 'Pay tax bill',      0, 3, 0, 0, 1714900000.0, 1714900100.0);

        -- One anytime to-do (start=1) inside a project, just to ensure inbox query excludes it.
        INSERT INTO TMTask
            (uuid, title, type, status, trashed, start, project, creationDate, userModificationDate)
        VALUES
            ('todo-4', 'Read RFC 9457', 0, 0, 0, 1, 'proj-1', 1715001000.0, 1715001100.0);

        -- A project.
        INSERT INTO TMTask
            (uuid, title, type, status, trashed, start, area, creationDate, userModificationDate)
        VALUES
            ('proj-1', 'Reading list', 1, 0, 0, 1, 'area-1', 1714000000.0, 1714000100.0);

        -- Tag mapping: todo-2 carries 'Errand'.
        INSERT INTO TMTaskTag (tasks, tags) VALUES ('todo-2', 'tag-errand');
    "#)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn fixture_has_expected_inbox_rows() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("test.sqlite");
        build_fixture(&path).unwrap();
        let c = Connection::open(&path).unwrap();
        let n: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM TMTask WHERE start = 0 AND trashed = 0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 3);
    }
}
