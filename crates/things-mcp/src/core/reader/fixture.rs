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
        CREATE TABLE TMChecklistItem (
            uuid TEXT PRIMARY KEY,
            title TEXT,
            status INTEGER,
            task TEXT,
            "index" INTEGER,
            stopDate REAL,
            creationDate REAL,
            userModificationDate REAL
        );
        CREATE TABLE Meta (key TEXT PRIMARY KEY, value TEXT);

        INSERT INTO Meta (key, value) VALUES ('databaseVersion', '21');

        -- Two areas, deliberately not in alphabetical order so ORDER BY "index" matters.
        INSERT INTO TMArea (uuid, title, "index") VALUES
            ('area-1', 'Personal', 0),
            ('area-2', 'Work',     1);

        -- Tags: 'Call' is a child of 'Errand'; 'Deep work' is a sibling with a shortcut.
        INSERT INTO TMTag (uuid, title, "index", shortcut, parent) VALUES
            ('tag-errand', 'Errand',    0, NULL, NULL),
            ('tag-call',   'Call',      0, NULL, 'tag-errand'),
            ('tag-deep',   'Deep work', 1, 'D',  NULL);

        -- Inbox to-dos: 2 open, 1 completed (status=3).
        INSERT INTO TMTask
            (uuid, title, type, status, trashed, start, creationDate, userModificationDate)
        VALUES
            ('todo-1', 'Buy milk',          0, 0, 0, 0, 1715000000.0, 1715000100.0),
            ('todo-2', 'Call the dentist',  0, 0, 0, 0, 1715000200.0, 1715000300.0),
            ('todo-3', 'Pay tax bill',      0, 3, 0, 0, 1714900000.0, 1714900100.0);

        -- Anytime to-do, scheduled far in the past so list_today always picks it up.
        -- pack_things_date(2020, 1, 1) = (2020<<16) | (1<<12) | (1<<7) = 132386944
        INSERT INTO TMTask
            (uuid, title, type, status, trashed, start, startDate, project, creationDate, userModificationDate, todayIndex)
        VALUES
            ('todo-today', 'Today scheduled item', 0, 0, 0, 1, 132386944, 'proj-1', 1715000600.0, 1715000700.0, 0);

        -- Anytime to-do scheduled far in the future (upcoming).
        -- pack_things_date(2099, 12, 31) = (2099<<16) | (12<<12) | (31<<7) = 137613184
        INSERT INTO TMTask
            (uuid, title, type, status, trashed, start, startDate, area, creationDate, userModificationDate)
        VALUES
            ('todo-upcoming-sched', 'Upcoming scheduled item', 0, 0, 0, 1, 137613184, 'area-2', 1715001100.0, 1715001200.0);

        -- Anytime to-do with a far-future deadline but no scheduled date —
        -- appears in BOTH list_anytime and list_upcoming (mirrors Things UI).
        INSERT INTO TMTask
            (uuid, title, type, status, trashed, start, deadline, area, creationDate, userModificationDate)
        VALUES
            ('todo-upcoming-dl', 'Upcoming deadlined item', 0, 0, 0, 1, 137613184, 'area-1', 1715001300.0, 1715001400.0);

        -- Plain anytime to-do (no scheduled date, no deadline) inside proj-1.
        INSERT INTO TMTask
            (uuid, title, type, status, trashed, start, project, creationDate, userModificationDate)
        VALUES
            ('todo-4', 'Read RFC 9457', 0, 0, 0, 1, 'proj-1', 1715001000.0, 1715001100.0);

        -- Someday to-do, in area-2.
        INSERT INTO TMTask
            (uuid, title, type, status, trashed, start, area, creationDate, userModificationDate)
        VALUES
            ('todo-someday', 'Read research papers', 0, 0, 0, 2, 'area-2', 1715002000.0, 1715002100.0);

        -- Logbook items: one completed (status=3), one canceled (status=2), both with stopDate.
        INSERT INTO TMTask
            (uuid, title, type, status, trashed, start, stopDate, creationDate, userModificationDate)
        VALUES
            ('todo-log-1', 'Old completed', 0, 3, 0, 1, 1714000000.0, 1713000000.0, 1714000000.0),
            ('todo-log-2', 'Old canceled',  0, 2, 0, 1, 1714500000.0, 1713500000.0, 1714500000.0);

        -- Trashed to-do (start=0 would put it in inbox, but trashed=1 hides it everywhere except list_trash).
        INSERT INTO TMTask
            (uuid, title, type, status, trashed, start, creationDate, userModificationDate)
        VALUES
            ('todo-trash', 'Trashed thing', 0, 0, 1, 0, 1714800000.0, 1714800100.0);

        -- Two projects: proj-1 (open) in area-1, proj-2 (done) in area-2.
        INSERT INTO TMTask
            (uuid, title, type, status, trashed, start, area, notes, creationDate, userModificationDate)
        VALUES
            ('proj-1', 'Reading list', 1, 0, 0, 1, 'area-1', 'Track what to read next', 1714000000.0, 1714000100.0),
            ('proj-2', 'Shipped Q1',   1, 3, 0, 1, 'area-2', NULL,                      1714100000.0, 1714100100.0);

        -- A heading under proj-1 with one to-do beneath it.
        INSERT INTO TMTask
            (uuid, title, type, status, trashed, start, project, "index", creationDate, userModificationDate)
        VALUES
            ('head-1', 'Articles', 2, 0, 0, 1, 'proj-1', 1, 1714000500.0, 1714000600.0);

        INSERT INTO TMTask
            (uuid, title, type, status, trashed, start, project, heading, "index", creationDate, userModificationDate)
        VALUES
            ('todo-in-head', 'Read intro', 0, 0, 0, 1, 'proj-1', 'head-1', 2, 1714000700.0, 1714000800.0);

        -- Checklist items for todo-1: 2 open, 1 completed (status=3).
        INSERT INTO TMChecklistItem
            (uuid, title, status, task, "index", creationDate, userModificationDate)
        VALUES
            ('chk-1', 'Walk to shop',   0, 'todo-1', 0, 1715000000.0, 1715000050.0),
            ('chk-2', 'Buy whole milk', 0, 'todo-1', 1, 1715000010.0, 1715000060.0),
            ('chk-3', 'Pay with card',  3, 'todo-1', 2, 1715000020.0, 1715000070.0);

        -- Tag mappings:
        --   todo-2          → 'Errand' (parent tag)        (existing)
        --   todo-4          → 'Call'   (child of 'Errand') — exercises list_by_tag recursion
        --   todo-someday    → 'Deep work'
        --   proj-1          → 'Errand' (projects can carry tags too)
        INSERT INTO TMTaskTag (tasks, tags) VALUES
            ('todo-2',       'tag-errand'),
            ('todo-4',       'tag-call'),
            ('todo-someday', 'tag-deep'),
            ('proj-1',       'tag-errand');
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
