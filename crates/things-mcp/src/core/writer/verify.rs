//! Post-write verification: poll the reader pool until we see the expected
//! change (verified), the predicate proves the row will never exist
//! (NotFound — for updates and status-changes only), or the timeout elapses
//! (Timeout). Bounded `poll_timeout / poll_interval` so a misbehaving Things
//! cannot hang the MCP call.

use std::time::{Duration, Instant};

use rusqlite::Connection;

use crate::core::error::ThingsError;
use crate::core::reader::pool::ReaderPool;
use crate::core::types::{TaskKind, TaskStatus, TodoSummary};

#[derive(Debug, Clone)]
pub enum VerifyPredicate {
    /// A row with this title and creationDate ≥ since_unix should exist.
    /// `kind` selects the row's `type` column: Todo → 0, Project → 1.
    CreateByTitle {
        title: String,
        since_unix: f64,
        kind: TaskKind,
    },
    /// The row at this id should match all populated expected_* fields.
    UpdateById {
        id: String,
        expected_title: Option<String>,
        expected_notes: Option<String>,
    },
    /// The row at this id should have this status.
    StatusChange { id: String, want: TaskStatus },
    /// The row at this id should have its project/area column set to the
    /// expected_list_id. `Some(uuid)` matches when t.project = uuid OR
    /// t.area = uuid. `None` matches when BOTH columns are NULL (the inbox).
    MoveById {
        id: String,
        expected_list_id: Option<String>,
    },
    /// The row at `id` either does or doesn't have the named tag, depending
    /// on `present`. Used by `things_assign_tag` (`present: true`) and
    /// `things_unassign_tag` (`present: false`). Tag matched by title via
    /// the TMTaskTag→TMTag join.
    TagOnTodoById {
        id: String,
        tag: String,
        present: bool,
    },
}

#[derive(Debug)]
pub enum VerifyOutcome {
    Verified { row: TodoSummary, latency_ms: u64 },
    Timeout { latency_ms: u64 },
    /// Only emitted by UpdateById / StatusChange when the row never exists.
    /// Plan 4 wires this through `WriteOutcome { verified: false, id: None }`.
    NotFound { latency_ms: u64 },
}

pub async fn verify(
    pool: &ReaderPool,
    pred: VerifyPredicate,
    timeout: Duration,
    interval: Duration,
) -> Result<VerifyOutcome, ThingsError> {
    let start = Instant::now();

    // For UpdateById / StatusChange the row should already exist in the DB —
    // if it never does, no amount of polling will help, so short-circuit.
    if let VerifyPredicate::UpdateById { id, .. }
        | VerifyPredicate::StatusChange { id, .. }
        | VerifyPredicate::MoveById { id, .. }
        | VerifyPredicate::TagOnTodoById { id, .. } = &pred
    {
        let id_for_probe = id.clone();
        let exists = pool
            .with_conn(move |c| -> rusqlite::Result<bool> {
                c.query_row(
                    "SELECT EXISTS (SELECT 1 FROM TMTask WHERE uuid = ? AND trashed = 0)",
                    rusqlite::params![id_for_probe],
                    |r| r.get::<_, i64>(0).map(|n| n != 0),
                )
            })
            .await?;
        if !exists {
            return Ok(VerifyOutcome::NotFound {
                latency_ms: start.elapsed().as_millis() as u64,
            });
        }
    }

    loop {
        let pred_clone = pred.clone();
        let found = pool
            .with_conn(move |c| check_once(c, &pred_clone))
            .await?;
        if let Some(row) = found {
            return Ok(VerifyOutcome::Verified {
                row,
                latency_ms: start.elapsed().as_millis() as u64,
            });
        }
        if start.elapsed() >= timeout {
            return Ok(VerifyOutcome::Timeout {
                latency_ms: start.elapsed().as_millis() as u64,
            });
        }
        tokio::time::sleep(interval).await;
    }
}

fn check_once(c: &Connection, pred: &VerifyPredicate) -> rusqlite::Result<Option<TodoSummary>> {
    use crate::core::reader::queries::{row_to_summary, SUMMARY_COLS, SUMMARY_COLS_LEN};
    match pred {
        VerifyPredicate::CreateByTitle { title, since_unix, kind } => {
            let type_int: i64 = match kind {
                TaskKind::Todo => 0,
                TaskKind::Project => 1,
                TaskKind::Heading => 2,
            };
            let sql = format!(
                r#"
                SELECT {SUMMARY_COLS}
                FROM TMTask AS t
                WHERE t.trashed = 0
                  AND t.type = ?
                  AND t.title = ?
                  AND t.creationDate >= ?
                ORDER BY t.creationDate DESC
                LIMIT 1
                "#
            );
            let mut stmt = c.prepare_cached(&sql)?;
            let mut rows = stmt.query(rusqlite::params![type_int, title, since_unix])?;
            if let Some(r) = rows.next()? {
                return row_to_summary(r).map(Some);
            }
            Ok(None)
        }
        VerifyPredicate::UpdateById {
            id,
            expected_title,
            expected_notes,
        } => {
            let sql = format!(
                r#"
                SELECT {SUMMARY_COLS}, t.notes
                FROM TMTask AS t
                WHERE t.uuid = ? AND t.trashed = 0
                LIMIT 1
                "#
            );
            let mut stmt = c.prepare_cached(&sql)?;
            let mut rows = stmt.query(rusqlite::params![id])?;
            let Some(r) = rows.next()? else {
                return Ok(None);
            };
            let summary = row_to_summary(r)?;
            let notes: Option<String> = r.get(SUMMARY_COLS_LEN)?;
            if let Some(want) = expected_title.as_ref() {
                if summary.title != *want {
                    return Ok(None);
                }
            }
            if let Some(want) = expected_notes.as_ref() {
                if notes.as_deref() != Some(want.as_str()) {
                    return Ok(None);
                }
            }
            Ok(Some(summary))
        }
        VerifyPredicate::StatusChange { id, want } => {
            let sql = format!(
                r#"
                SELECT {SUMMARY_COLS}
                FROM TMTask AS t
                WHERE t.uuid = ? AND t.trashed = 0
                LIMIT 1
                "#
            );
            let mut stmt = c.prepare_cached(&sql)?;
            let mut rows = stmt.query(rusqlite::params![id])?;
            let Some(r) = rows.next()? else {
                return Ok(None);
            };
            let summary = row_to_summary(r)?;
            if summary.status == *want {
                Ok(Some(summary))
            } else {
                Ok(None)
            }
        }
        VerifyPredicate::MoveById { id, expected_list_id } => {
            let sql = format!(
                r#"
                SELECT {SUMMARY_COLS}
                FROM TMTask AS t
                WHERE t.uuid = ? AND t.trashed = 0
                LIMIT 1
                "#
            );
            let mut stmt = c.prepare_cached(&sql)?;
            let mut rows = stmt.query(rusqlite::params![id])?;
            let Some(r) = rows.next()? else {
                return Ok(None);
            };
            let summary = row_to_summary(r)?;
            let matches = match expected_list_id.as_deref() {
                None => summary.project_id.is_none() && summary.area_id.is_none(),
                Some(want) => {
                    summary.project_id.as_deref() == Some(want)
                        || summary.area_id.as_deref() == Some(want)
                }
            };
            if matches {
                Ok(Some(summary))
            } else {
                Ok(None)
            }
        }
        VerifyPredicate::TagOnTodoById { id, tag, present } => {
            // Tag-presence join: TMTaskTag.tasks = task uuid, TMTaskTag.tags
            // = tag uuid, TMTag.title = tag's user-facing name.
            let has_tag_sql = r#"
                SELECT EXISTS (
                    SELECT 1
                    FROM TMTaskTag AS tt
                    JOIN TMTag      AS g  ON g.uuid = tt.tags
                    WHERE tt.tasks = ? AND g.title = ?
                )
            "#;
            let mut stmt = c.prepare_cached(has_tag_sql)?;
            let has_tag: bool = stmt
                .query_row(rusqlite::params![id, tag], |r| {
                    r.get::<_, i64>(0).map(|n| n != 0)
                })?;
            if has_tag != *present {
                return Ok(None);
            }
            // Emit a summary just like the other arms do.
            let summary_sql = format!(
                r#"
                SELECT {SUMMARY_COLS}
                FROM TMTask AS t
                WHERE t.uuid = ? AND t.trashed = 0
                LIMIT 1
                "#
            );
            let mut summary_stmt = c.prepare_cached(&summary_sql)?;
            let mut rows = summary_stmt.query(rusqlite::params![id])?;
            let Some(r) = rows.next()? else { return Ok(None) };
            row_to_summary(r).map(Some)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reader::fixture::build_fixture;
    use tempfile::tempdir;

    fn cfg() -> (Duration, Duration) {
        (Duration::from_millis(500), Duration::from_millis(20))
    }

    async fn open_pool() -> (tempfile::TempDir, ReaderPool) {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        (tmp, pool)
    }

    #[tokio::test]
    async fn verify_create_by_title_finds_existing_row() {
        let (_tmp, pool) = open_pool().await;
        let (timeout, interval) = cfg();
        // The fixture seeds 'Buy milk' with creationDate=1715000000.0.
        let out = verify(
            &pool,
            VerifyPredicate::CreateByTitle {
                title: "Buy milk".into(),
                since_unix: 0.0,
                kind: TaskKind::Todo,
            },
            timeout,
            interval,
        )
        .await
        .unwrap();
        match out {
            VerifyOutcome::Verified { row, .. } => assert_eq!(row.title, "Buy milk"),
            other => panic!("expected Verified, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn verify_create_by_title_times_out_when_title_absent() {
        let (_tmp, pool) = open_pool().await;
        let (timeout, interval) = cfg();
        let out = verify(
            &pool,
            VerifyPredicate::CreateByTitle {
                title: "Nothing in the fixture matches this".into(),
                since_unix: 0.0,
                kind: TaskKind::Todo,
            },
            timeout,
            interval,
        )
        .await
        .unwrap();
        assert!(matches!(out, VerifyOutcome::Timeout { .. }));
    }

    #[tokio::test]
    async fn verify_update_by_id_matches_when_fields_align() {
        let (_tmp, pool) = open_pool().await;
        let (timeout, interval) = cfg();
        let out = verify(
            &pool,
            VerifyPredicate::UpdateById {
                id: "todo-1".into(),
                expected_title: Some("Buy milk".into()),
                expected_notes: None,
            },
            timeout,
            interval,
        )
        .await
        .unwrap();
        match out {
            VerifyOutcome::Verified { row, .. } => assert_eq!(row.id, "todo-1"),
            other => panic!("expected Verified, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn verify_update_by_id_not_found_short_circuits() {
        let (_tmp, pool) = open_pool().await;
        let (timeout, interval) = cfg();
        let start = std::time::Instant::now();
        let out = verify(
            &pool,
            VerifyPredicate::UpdateById {
                id: "does-not-exist".into(),
                expected_title: None,
                expected_notes: None,
            },
            timeout,
            interval,
        )
        .await
        .unwrap();
        assert!(matches!(out, VerifyOutcome::NotFound { .. }));
        // Must short-circuit well under the timeout.
        assert!(
            start.elapsed() < Duration::from_millis(200),
            "expected NotFound to short-circuit, took {:?}",
            start.elapsed()
        );
    }

    #[tokio::test]
    async fn verify_status_change_matches_when_status_equals_want() {
        let (_tmp, pool) = open_pool().await;
        let (timeout, interval) = cfg();
        // The fixture's 'todo-3 Pay tax bill' has SQLite status=3 → TaskStatus::Completed.
        let out = verify(
            &pool,
            VerifyPredicate::StatusChange {
                id: "todo-3".into(),
                want: TaskStatus::Completed,
            },
            timeout,
            interval,
        )
        .await
        .unwrap();
        match out {
            VerifyOutcome::Verified { row, .. } => assert_eq!(row.id, "todo-3"),
            other => panic!("expected Verified, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn verify_move_by_id_matches_when_row_under_expected_list() {
        let (_tmp, pool) = open_pool().await;
        let (timeout, interval) = cfg();
        // The fixture's todo-4 lives under project proj-1.
        let out = verify(
            &pool,
            VerifyPredicate::MoveById {
                id: "todo-4".into(),
                expected_list_id: Some("proj-1".into()),
            },
            timeout,
            interval,
        )
        .await
        .unwrap();
        match out {
            VerifyOutcome::Verified { row, .. } => {
                assert_eq!(row.id, "todo-4");
                assert_eq!(row.project_id.as_deref(), Some("proj-1"));
            }
            other => panic!("expected Verified, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn verify_move_by_id_inbox_matches_when_both_parent_columns_null() {
        let (_tmp, pool) = open_pool().await;
        let (timeout, interval) = cfg();
        // The fixture's todo-1 ('Buy milk') has no project + no area.
        let out = verify(
            &pool,
            VerifyPredicate::MoveById {
                id: "todo-1".into(),
                expected_list_id: None,
            },
            timeout,
            interval,
        )
        .await
        .unwrap();
        assert!(matches!(out, VerifyOutcome::Verified { .. }));
    }

    #[tokio::test]
    async fn verify_tag_on_todo_by_id_matches_when_present_true_and_tag_set() {
        let (_tmp, pool) = open_pool().await;
        let (timeout, interval) = cfg();
        // Fixture: todo-2 carries the 'Errand' tag.
        let out = verify(
            &pool,
            VerifyPredicate::TagOnTodoById {
                id: "todo-2".into(),
                tag: "Errand".into(),
                present: true,
            },
            timeout,
            interval,
        )
        .await
        .unwrap();
        match out {
            VerifyOutcome::Verified { row, .. } => assert_eq!(row.id, "todo-2"),
            other => panic!("expected Verified, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn verify_tag_on_todo_by_id_matches_when_present_false_and_tag_absent() {
        let (_tmp, pool) = open_pool().await;
        let (timeout, interval) = cfg();
        // Fixture: todo-1 ('Buy milk') has no tags.
        let out = verify(
            &pool,
            VerifyPredicate::TagOnTodoById {
                id: "todo-1".into(),
                tag: "Errand".into(),
                present: false,
            },
            timeout,
            interval,
        )
        .await
        .unwrap();
        assert!(matches!(out, VerifyOutcome::Verified { .. }));
    }
}
