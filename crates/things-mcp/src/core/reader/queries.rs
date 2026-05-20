//! Typed SQL helpers against the live Things schema. Every query goes through
//! `prepare_cached`; no string interpolation of user input.
//!
//! Date semantics:
//! - `creationDate`, `userModificationDate`, `stopDate` are REAL Unix seconds.
//! - `startDate`, `deadline` are bit-packed integers (handled in later tasks).

use crate::core::error::ThingsError;
use crate::core::reader::pool::ReaderPool;
use crate::core::types::{StartBucket, TaskStatus, TodoSummary};

/// Standard `TodoSummary`-shaped column projection used by every list query.
/// SQL must `SELECT` columns in this exact order:
///
/// `t.uuid, t.title, t.status, t.start, t.project, t.area, t.heading,
///  t.startDate, t.deadline, t.creationDate, t.userModificationDate`
pub(crate) const SUMMARY_COLS: &str =
    "t.uuid, t.title, t.status, t.start, t.project, t.area, t.heading, \
     t.startDate, t.deadline, t.creationDate, t.userModificationDate";

pub(crate) fn row_to_summary(r: &rusqlite::Row<'_>) -> rusqlite::Result<TodoSummary> {
    use crate::core::reader::dates::decode_things_date;
    Ok(TodoSummary {
        id: r.get::<_, String>(0)?,
        title: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
        status: TaskStatus::from_sqlite(r.get::<_, i64>(2)?),
        start: StartBucket::from_sqlite(r.get::<_, i64>(3)?),
        project_id: r.get::<_, Option<String>>(4)?,
        area_id: r.get::<_, Option<String>>(5)?,
        heading_id: r.get::<_, Option<String>>(6)?,
        tags: Vec::new(),
        scheduled: r.get::<_, Option<i64>>(7)?.and_then(decode_things_date),
        deadline: r.get::<_, Option<i64>>(8)?.and_then(decode_things_date),
        creation_date: r.get::<_, Option<f64>>(9)?.map(unix_to_iso),
        modification_date: r.get::<_, Option<f64>>(10)?.map(unix_to_iso),
    })
}

pub struct ListInboxParams {
    pub include_completed: bool,
    pub limit: u32,
}

impl Default for ListInboxParams {
    fn default() -> Self {
        Self {
            include_completed: false,
            limit: 200,
        }
    }
}

pub async fn list_inbox(
    pool: &ReaderPool,
    params: ListInboxParams,
) -> Result<Vec<TodoSummary>, ThingsError> {
    let status_filter: &'static str = if params.include_completed {
        ""
    } else {
        " AND status = 0"
    };
    let sql = format!(
        r#"
        SELECT {SUMMARY_COLS}
        FROM TMTask AS t
        WHERE t.trashed = 0
          AND t.type = 0
          AND t.start = 0
          {status_filter}
        ORDER BY t.creationDate DESC
        LIMIT ?1
        "#,
    );
    let limit = params.limit as i64;
    let rows = pool
        .with_conn(move |c| -> rusqlite::Result<Vec<TodoSummary>> {
            let mut stmt = c.prepare_cached(&sql)?;
            let iter = stmt.query_map([limit], row_to_summary)?;
            iter.collect()
        })
        .await?;
    let ids: Vec<String> = rows.iter().map(|r| r.id.clone()).collect();
    let tag_map = fetch_tags_for_tasks(pool, ids).await?;
    let mut with_tags = rows;
    for row in with_tags.iter_mut() {
        if let Some(v) = tag_map.get(&row.id) {
            row.tags = v.clone();
        }
    }
    Ok(with_tags)
}

async fn fetch_tags_for_tasks(
    pool: &ReaderPool,
    task_ids: Vec<String>,
) -> Result<std::collections::HashMap<String, Vec<String>>, ThingsError> {
    if task_ids.is_empty() {
        return Ok(Default::default());
    }
    let placeholders = (0..task_ids.len())
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        r#"
        SELECT tt.tasks, tg.title
        FROM TMTaskTag AS tt
        JOIN TMTag AS tg ON tg.uuid = tt.tags
        WHERE tt.tasks IN ({placeholders})
        ORDER BY tt.tasks, tg.title
        "#,
    );
    let pairs = pool
        .with_conn(move |c| -> rusqlite::Result<Vec<(String, String)>> {
            let mut stmt = c.prepare_cached(&sql)?;
            let params = rusqlite::params_from_iter(task_ids.iter());
            let iter = stmt.query_map(params, |r| Ok((r.get(0)?, r.get(1)?)))?;
            iter.collect()
        })
        .await?;
    let mut out: std::collections::HashMap<String, Vec<String>> = Default::default();
    for (task, tag) in pairs {
        out.entry(task).or_default().push(tag);
    }
    Ok(out)
}

fn unix_to_iso(secs: f64) -> String {
    // Minimal ISO-8601 emitter so we don't pull in `chrono` for one helper.
    let s = secs as i64;
    let (y, mo, d, h, mi, sec) = crate::core::backup::unix_to_ymdhms(s);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{sec:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reader::{fixture::build_fixture, pool::ReaderPool};
    use tempfile::tempdir;

    #[tokio::test]
    async fn list_inbox_default_excludes_completed() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let rows = list_inbox(&pool, ListInboxParams::default()).await.unwrap();
        // fixture: 3 inbox rows, one of which is status=3 (completed)
        assert_eq!(rows.len(), 2);
        let titles: Vec<_> = rows.iter().map(|r| r.title.as_str()).collect();
        assert!(titles.contains(&"Buy milk"));
        assert!(titles.contains(&"Call the dentist"));
    }

    #[tokio::test]
    async fn list_inbox_with_completed_includes_completed() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let rows = list_inbox(
            &pool,
            ListInboxParams {
                include_completed: true,
                limit: 200,
            },
        )
        .await
        .unwrap();
        assert_eq!(rows.len(), 3);
    }

    #[tokio::test]
    async fn list_inbox_attaches_tags() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let rows = list_inbox(&pool, ListInboxParams::default()).await.unwrap();
        let dentist = rows.iter().find(|r| r.title == "Call the dentist").unwrap();
        assert_eq!(dentist.tags, vec!["Errand".to_string()]);
    }
}
