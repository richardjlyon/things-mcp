//! Typed SQL helpers against the live Things schema. Every query goes through
//! `prepare_cached`; no string interpolation of user input.
//!
//! Date semantics:
//! - `creationDate`, `userModificationDate`, `stopDate` are REAL Unix seconds.
//! - `startDate`, `deadline` are bit-packed integers (handled in later tasks).

use crate::core::error::ThingsError;
use crate::core::reader::pool::ReaderPool;
use crate::core::types::{
    Area, ChecklistItem, Project, StartBucket, Tag, TaskStatus, TodoFull, TodoSummary,
};

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
    attach_tags(pool, rows).await
}

pub struct ListTodayParams {
    pub limit: u32,
}

impl Default for ListTodayParams {
    fn default() -> Self {
        Self { limit: 200 }
    }
}

pub async fn list_today(
    pool: &ReaderPool,
    params: ListTodayParams,
) -> Result<Vec<TodoSummary>, ThingsError> {
    use crate::core::reader::dates::today_packed_utc;
    let today = today_packed_utc();
    let sql = format!(
        r#"
        SELECT {SUMMARY_COLS}
        FROM TMTask AS t
        WHERE t.trashed = 0
          AND t.type = 0
          AND t.status = 0
          AND t.start = 1
          AND t.startDate > 0
          AND t.startDate <= ?1
        ORDER BY t.todayIndex IS NULL, t.todayIndex, t.userModificationDate DESC
        LIMIT ?2
        "#,
    );
    let limit = params.limit as i64;
    let rows = pool
        .with_conn(move |c| -> rusqlite::Result<Vec<TodoSummary>> {
            let mut stmt = c.prepare_cached(&sql)?;
            let iter = stmt.query_map([today, limit], row_to_summary)?;
            iter.collect()
        })
        .await?;
    attach_tags(pool, rows).await
}

pub struct ListUpcomingParams {
    pub from_iso: Option<String>,
    pub to_iso: Option<String>,
    pub limit: u32,
}

impl Default for ListUpcomingParams {
    fn default() -> Self {
        Self {
            from_iso: None,
            to_iso: None,
            limit: 200,
        }
    }
}

pub async fn list_upcoming(
    pool: &ReaderPool,
    params: ListUpcomingParams,
) -> Result<Vec<TodoSummary>, ThingsError> {
    use crate::core::reader::dates::{pack_things_date, parse_iso_date, today_packed_utc};

    let lower = match params.from_iso.as_deref() {
        None => today_packed_utc(),
        Some(s) => parse_iso_date(s)
            .map(|(y, m, d)| pack_things_date(y, m, d))
            .ok_or_else(|| ThingsError::InvalidInput {
                field: "from".into(),
                reason: format!("expected YYYY-MM-DD, got {s:?}"),
            })?,
    };
    let upper: i64 = match params.to_iso.as_deref() {
        None => i64::MAX,
        Some(s) => parse_iso_date(s)
            .map(|(y, m, d)| pack_things_date(y, m, d))
            .ok_or_else(|| ThingsError::InvalidInput {
                field: "to".into(),
                reason: format!("expected YYYY-MM-DD, got {s:?}"),
            })?,
    };

    let sql = format!(
        r#"
        SELECT {SUMMARY_COLS}
        FROM TMTask AS t
        WHERE t.trashed = 0
          AND t.type = 0
          AND t.status = 0
          AND (
                (t.startDate > 0 AND t.startDate > ?1 AND t.startDate <= ?2)
             OR (t.deadline  > 0 AND t.deadline  > ?1 AND t.deadline  <= ?2)
          )
        ORDER BY
            CASE
                WHEN t.startDate > 0 AND t.deadline > 0 THEN MIN(t.startDate, t.deadline)
                WHEN t.startDate > 0                    THEN t.startDate
                ELSE t.deadline
            END
        LIMIT ?3
        "#,
    );
    let limit = params.limit as i64;
    let rows = pool
        .with_conn(move |c| -> rusqlite::Result<Vec<TodoSummary>> {
            let mut stmt = c.prepare_cached(&sql)?;
            let iter = stmt.query_map([lower, upper, limit], row_to_summary)?;
            iter.collect()
        })
        .await?;
    attach_tags(pool, rows).await
}

pub struct ListAnytimeParams {
    pub area_id: Option<String>,
    pub limit: u32,
}

impl Default for ListAnytimeParams {
    fn default() -> Self {
        Self {
            area_id: None,
            limit: 200,
        }
    }
}

pub async fn list_anytime(
    pool: &ReaderPool,
    params: ListAnytimeParams,
) -> Result<Vec<TodoSummary>, ThingsError> {
    let sql = format!(
        r#"
        SELECT {SUMMARY_COLS}
        FROM TMTask AS t
        LEFT JOIN TMTask AS p
               ON p.uuid = t.project AND p.type = 1
        WHERE t.trashed = 0
          AND t.type = 0
          AND t.status = 0
          AND t.start = 1
          AND (t.startDate IS NULL OR t.startDate = 0)
          AND (?1 IS NULL OR t.area = ?1 OR p.area = ?1)
        ORDER BY t.userModificationDate DESC
        LIMIT ?2
        "#,
    );
    let limit = params.limit as i64;
    let area = params.area_id;
    let rows = pool
        .with_conn(move |c| -> rusqlite::Result<Vec<TodoSummary>> {
            let mut stmt = c.prepare_cached(&sql)?;
            let iter = stmt.query_map(
                rusqlite::params![area, limit],
                row_to_summary,
            )?;
            iter.collect()
        })
        .await?;
    attach_tags(pool, rows).await
}

pub struct ListSomedayParams {
    pub limit: u32,
}

impl Default for ListSomedayParams {
    fn default() -> Self {
        Self { limit: 200 }
    }
}

pub async fn list_someday(
    pool: &ReaderPool,
    params: ListSomedayParams,
) -> Result<Vec<TodoSummary>, ThingsError> {
    let sql = format!(
        r#"
        SELECT {SUMMARY_COLS}
        FROM TMTask AS t
        WHERE t.trashed = 0
          AND t.type = 0
          AND t.status = 0
          AND t.start = 2
        ORDER BY t.userModificationDate DESC
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
    attach_tags(pool, rows).await
}

pub struct ListLogbookParams {
    pub from_iso: Option<String>,
    pub to_iso: Option<String>,
    pub limit: u32,
}

impl Default for ListLogbookParams {
    fn default() -> Self {
        Self {
            from_iso: None,
            to_iso: None,
            limit: 100,
        }
    }
}

pub async fn list_logbook(
    pool: &ReaderPool,
    params: ListLogbookParams,
) -> Result<Vec<TodoSummary>, ThingsError> {
    use crate::core::reader::dates::{parse_iso_date, ymd_to_unix_utc};
    let from_unix: Option<f64> = match params.from_iso.as_deref() {
        None => None,
        Some(s) => Some(
            parse_iso_date(s)
                .map(|(y, m, d)| ymd_to_unix_utc(y, m, d) as f64)
                .ok_or_else(|| ThingsError::InvalidInput {
                    field: "from".into(),
                    reason: format!("expected YYYY-MM-DD, got {s:?}"),
                })?,
        ),
    };
    let to_unix: Option<f64> = match params.to_iso.as_deref() {
        None => None,
        Some(s) => Some(
            parse_iso_date(s)
                // End of the requested day, exclusive of the next day.
                .map(|(y, m, d)| (ymd_to_unix_utc(y, m, d) + 86_400) as f64)
                .ok_or_else(|| ThingsError::InvalidInput {
                    field: "to".into(),
                    reason: format!("expected YYYY-MM-DD, got {s:?}"),
                })?,
        ),
    };

    let sql = format!(
        r#"
        SELECT {SUMMARY_COLS}
        FROM TMTask AS t
        WHERE t.trashed = 0
          AND t.type = 0
          AND t.status IN (2, 3)
          AND (?1 IS NULL OR t.stopDate >= ?1)
          AND (?2 IS NULL OR t.stopDate <  ?2)
        ORDER BY t.stopDate DESC
        LIMIT ?3
        "#,
    );
    let limit = params.limit as i64;
    let rows = pool
        .with_conn(move |c| -> rusqlite::Result<Vec<TodoSummary>> {
            let mut stmt = c.prepare_cached(&sql)?;
            let iter = stmt.query_map(
                rusqlite::params![from_unix, to_unix, limit],
                row_to_summary,
            )?;
            iter.collect()
        })
        .await?;
    attach_tags(pool, rows).await
}

pub struct ListTrashParams {
    pub limit: u32,
}

impl Default for ListTrashParams {
    fn default() -> Self {
        Self { limit: 100 }
    }
}

pub async fn list_trash(
    pool: &ReaderPool,
    params: ListTrashParams,
) -> Result<Vec<TodoSummary>, ThingsError> {
    let sql = format!(
        r#"
        SELECT {SUMMARY_COLS}
        FROM TMTask AS t
        WHERE t.trashed = 1
          AND t.type = 0
        ORDER BY t.userModificationDate DESC
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
    attach_tags(pool, rows).await
}

/// Helper used by every list query that returns `TodoSummary` rows.
async fn attach_tags(
    pool: &ReaderPool,
    mut rows: Vec<TodoSummary>,
) -> Result<Vec<TodoSummary>, ThingsError> {
    let ids: Vec<String> = rows.iter().map(|r| r.id.clone()).collect();
    let tag_map = fetch_tags_for_tasks(pool, ids).await?;
    for row in rows.iter_mut() {
        if let Some(v) = tag_map.get(&row.id) {
            row.tags = v.clone();
        }
    }
    Ok(rows)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectStatusFilter {
    Open,
    Done,
    All,
}

impl Default for ProjectStatusFilter {
    fn default() -> Self {
        Self::Open
    }
}

#[derive(Default)]
pub struct ListProjectsParams {
    pub area_id: Option<String>,
    pub status: ProjectStatusFilter,
}

pub async fn list_projects(
    pool: &ReaderPool,
    params: ListProjectsParams,
) -> Result<Vec<Project>, ThingsError> {
    let status_clause = match params.status {
        ProjectStatusFilter::Open => " AND t.status = 0",
        ProjectStatusFilter::Done => " AND t.status IN (2, 3)",
        ProjectStatusFilter::All => "",
    };
    let sql = format!(
        r#"
        SELECT t.uuid, t.title, t.area, t.status, t.notes
        FROM TMTask AS t
        WHERE t.trashed = 0
          AND t.type = 1
          AND (?1 IS NULL OR t.area = ?1)
          {status_clause}
        ORDER BY t.userModificationDate DESC
        "#,
    );
    let area = params.area_id;
    let rows = pool
        .with_conn(move |c| -> rusqlite::Result<Vec<Project>> {
            let mut stmt = c.prepare_cached(&sql)?;
            let iter = stmt.query_map(rusqlite::params![area], |r| {
                Ok(Project {
                    id: r.get::<_, String>(0)?,
                    title: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    area_id: r.get::<_, Option<String>>(2)?,
                    status: TaskStatus::from_sqlite(r.get::<_, i64>(3)?),
                    notes: r.get::<_, Option<String>>(4)?,
                    tags: Vec::new(),
                })
            })?;
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

pub async fn list_areas(pool: &ReaderPool) -> Result<Vec<Area>, ThingsError> {
    let sql = r#"
        SELECT a.uuid, a.title
        FROM TMArea AS a
        ORDER BY a."index", a.title
    "#;
    let rows = pool
        .with_conn(move |c| -> rusqlite::Result<Vec<Area>> {
            let mut stmt = c.prepare_cached(sql)?;
            let iter = stmt.query_map([], |r| {
                Ok(Area {
                    id: r.get::<_, String>(0)?,
                    title: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                })
            })?;
            iter.collect()
        })
        .await?;
    Ok(rows)
}

pub async fn list_tags(pool: &ReaderPool) -> Result<Vec<Tag>, ThingsError> {
    let sql = r#"
        SELECT g.uuid, g.title, g.parent, g.shortcut
        FROM TMTag AS g
        ORDER BY g."index", g.title
    "#;
    let rows = pool
        .with_conn(move |c| -> rusqlite::Result<Vec<Tag>> {
            let mut stmt = c.prepare_cached(sql)?;
            let iter = stmt.query_map([], |r| {
                Ok(Tag {
                    id: r.get::<_, String>(0)?,
                    title: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    parent_id: r.get::<_, Option<String>>(2)?,
                    shortcut: r.get::<_, Option<String>>(3)?,
                })
            })?;
            iter.collect()
        })
        .await?;
    Ok(rows)
}

pub async fn get_todo(
    pool: &ReaderPool,
    id: String,
) -> Result<Option<TodoFull>, ThingsError> {
    let id_for_summary = id.clone();
    let summary_sql = format!(
        r#"
        SELECT {SUMMARY_COLS}
        FROM TMTask AS t
        WHERE t.uuid = ?1 AND t.type = 0
        "#,
    );
    let detail_sql = r#"
        SELECT t.notes, t.stopDate, t.rt1_recurrenceRule IS NOT NULL AS is_repeating
        FROM TMTask AS t
        WHERE t.uuid = ?1 AND t.type = 0
    "#;
    let summary_opt = pool
        .with_conn(move |c| -> rusqlite::Result<Option<TodoSummary>> {
            let mut stmt = c.prepare_cached(&summary_sql)?;
            let mut rows = stmt.query([id_for_summary.as_str()])?;
            if let Some(row) = rows.next()? {
                Ok(Some(row_to_summary(row)?))
            } else {
                Ok(None)
            }
        })
        .await?;
    let summary = match summary_opt {
        Some(s) => s,
        None => return Ok(None),
    };

    let id_for_detail = id.clone();
    let (notes, completion_date, is_repeating) = pool
        .with_conn(move |c| -> rusqlite::Result<(Option<String>, Option<String>, bool)> {
            let mut stmt = c.prepare_cached(detail_sql)?;
            let mut rows = stmt.query([id_for_detail.as_str()])?;
            if let Some(row) = rows.next()? {
                let notes: Option<String> = row.get(0)?;
                let stop_date: Option<f64> = row.get(1)?;
                let is_repeating: bool = row.get::<_, i64>(2)? != 0;
                Ok((notes, stop_date.map(unix_to_iso), is_repeating))
            } else {
                Ok((None, None, false))
            }
        })
        .await?;

    let id_for_checklist = id.clone();
    let checklist = pool
        .with_conn(move |c| -> rusqlite::Result<Vec<ChecklistItem>> {
            let mut stmt = c.prepare_cached(
                r#"
                SELECT c.uuid, c.title, c.status
                FROM TMChecklistItem AS c
                WHERE c.task = ?1
                ORDER BY c."index"
                "#,
            )?;
            let iter = stmt.query_map([id_for_checklist.as_str()], |r| {
                Ok(ChecklistItem {
                    id: r.get::<_, String>(0)?,
                    title: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    status: TaskStatus::from_sqlite(r.get::<_, i64>(2)?),
                })
            })?;
            iter.collect()
        })
        .await?;

    // Attach tags onto the summary by reusing fetch_tags_for_tasks for one id.
    let tag_map = fetch_tags_for_tasks(pool, vec![id.clone()]).await?;
    let mut summary = summary;
    if let Some(v) = tag_map.get(&id) {
        summary.tags = v.clone();
    }

    Ok(Some(TodoFull {
        summary,
        notes,
        checklist,
        completion_date,
        is_repeating_template: is_repeating,
    }))
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

    #[tokio::test]
    async fn list_today_includes_past_scheduled() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let rows = list_today(&pool, ListTodayParams::default()).await.unwrap();
        let titles: Vec<_> = rows.iter().map(|r| r.title.as_str()).collect();
        assert!(titles.contains(&"Today scheduled item"));
        // Future-scheduled item must NOT be in Today.
        assert!(!titles.contains(&"Upcoming scheduled item"));
    }

    #[tokio::test]
    async fn list_upcoming_returns_future_scheduled_and_deadlined() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let rows = list_upcoming(&pool, ListUpcomingParams::default()).await.unwrap();
        let titles: Vec<_> = rows.iter().map(|r| r.title.as_str()).collect();
        assert!(titles.contains(&"Upcoming scheduled item"));
        assert!(titles.contains(&"Upcoming deadlined item"));
        // Today-scheduled and never-scheduled items must NOT be in Upcoming.
        assert!(!titles.contains(&"Today scheduled item"));
        assert!(!titles.contains(&"Read RFC 9457"));
    }

    #[tokio::test]
    async fn list_anytime_returns_unscheduled_anytime_items() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let rows = list_anytime(&pool, ListAnytimeParams::default()).await.unwrap();
        let titles: Vec<_> = rows.iter().map(|r| r.title.as_str()).collect();
        assert!(titles.contains(&"Read RFC 9457"));
        // Has a deadline but no scheduled date → still anytime.
        assert!(titles.contains(&"Upcoming deadlined item"));
        // Future-scheduled item is NOT anytime.
        assert!(!titles.contains(&"Upcoming scheduled item"));
        // Today-scheduled item is NOT anytime.
        assert!(!titles.contains(&"Today scheduled item"));
    }

    #[tokio::test]
    async fn list_anytime_area_filter() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let rows = list_anytime(
            &pool,
            ListAnytimeParams {
                area_id: Some("area-1".to_string()),
                limit: 200,
            },
        )
        .await
        .unwrap();
        let titles: Vec<_> = rows.iter().map(|r| r.title.as_str()).collect();
        // proj-1 is in area-1, so todo-4 inside proj-1 should be picked up via the project join.
        assert!(titles.contains(&"Read RFC 9457"));
        // todo-upcoming-dl has area=area-1 directly.
        assert!(titles.contains(&"Upcoming deadlined item"));
    }

    #[tokio::test]
    async fn list_someday_returns_start_2_items() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let rows = list_someday(&pool, ListSomedayParams::default()).await.unwrap();
        let titles: Vec<_> = rows.iter().map(|r| r.title.as_str()).collect();
        assert_eq!(rows.len(), 1);
        assert!(titles.contains(&"Read research papers"));
    }

    #[tokio::test]
    async fn list_logbook_returns_completed_and_canceled_ordered_by_stopdate() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let rows = list_logbook(&pool, ListLogbookParams::default()).await.unwrap();
        let titles: Vec<_> = rows.iter().map(|r| r.title.as_str()).collect();
        assert!(titles.contains(&"Old completed"));
        assert!(titles.contains(&"Old canceled"));
        // Older completion comes after newer one (DESC by stopDate).
        let pos_old = titles.iter().position(|t| *t == "Old completed").unwrap();
        let pos_newer = titles.iter().position(|t| *t == "Old canceled").unwrap();
        assert!(pos_newer < pos_old);
    }

    #[tokio::test]
    async fn list_logbook_from_bound_excludes_older_items() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        // Old completed has stopDate 1714000000 ≈ 2024-04-24; old canceled has 1714500000 ≈ 2024-04-30.
        // from = 2024-04-27 → only canceled survives.
        let rows = list_logbook(
            &pool,
            ListLogbookParams {
                from_iso: Some("2024-04-27".to_string()),
                to_iso: None,
                limit: 100,
            },
        )
        .await
        .unwrap();
        let titles: Vec<_> = rows.iter().map(|r| r.title.as_str()).collect();
        assert!(titles.contains(&"Old canceled"));
        assert!(!titles.contains(&"Old completed"));
    }

    #[tokio::test]
    async fn list_trash_returns_trashed_items() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let rows = list_trash(&pool, ListTrashParams::default()).await.unwrap();
        let titles: Vec<_> = rows.iter().map(|r| r.title.as_str()).collect();
        assert_eq!(rows.len(), 1);
        assert!(titles.contains(&"Trashed thing"));
    }

    #[tokio::test]
    async fn list_areas_returns_areas_in_index_order() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let rows = list_areas(&pool).await.unwrap();
        let titles: Vec<_> = rows.iter().map(|r| r.title.as_str()).collect();
        assert_eq!(titles, vec!["Personal", "Work"]);
        assert_eq!(rows[0].id, "area-1");
        assert_eq!(rows[1].id, "area-2");
    }

    #[tokio::test]
    async fn list_upcoming_respects_to_bound() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        // to=2050-01-01 should still include the 2099-dated items? No — 2050
        // < 2099, so they are excluded.
        let rows = list_upcoming(
            &pool,
            ListUpcomingParams {
                from_iso: None,
                to_iso: Some("2050-01-01".to_string()),
                limit: 200,
            },
        )
        .await
        .unwrap();
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn list_projects_default_returns_open_only() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let rows = list_projects(&pool, ListProjectsParams::default()).await.unwrap();
        let titles: Vec<_> = rows.iter().map(|r| r.title.as_str()).collect();
        assert!(titles.contains(&"Reading list"));
        assert!(!titles.contains(&"Shipped Q1"));
    }

    #[tokio::test]
    async fn list_projects_status_done_returns_completed_only() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let rows = list_projects(
            &pool,
            ListProjectsParams {
                area_id: None,
                status: ProjectStatusFilter::Done,
            },
        )
        .await
        .unwrap();
        let titles: Vec<_> = rows.iter().map(|r| r.title.as_str()).collect();
        assert_eq!(titles, vec!["Shipped Q1"]);
    }

    #[tokio::test]
    async fn list_tags_returns_flat_list_with_parent_links() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let rows = list_tags(&pool).await.unwrap();
        assert_eq!(rows.len(), 3);
        let errand = rows.iter().find(|t| t.title == "Errand").unwrap();
        let call = rows.iter().find(|t| t.title == "Call").unwrap();
        let deep = rows.iter().find(|t| t.title == "Deep work").unwrap();
        assert!(errand.parent_id.is_none());
        assert_eq!(call.parent_id.as_deref(), Some("tag-errand"));
        assert!(deep.parent_id.is_none());
        assert_eq!(deep.shortcut.as_deref(), Some("D"));
    }

    #[tokio::test]
    async fn list_projects_area_filter_and_tag_attachment() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let rows = list_projects(
            &pool,
            ListProjectsParams {
                area_id: Some("area-1".to_string()),
                status: ProjectStatusFilter::All,
            },
        )
        .await
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "Reading list");
        assert_eq!(rows[0].tags, vec!["Errand".to_string()]);
    }

    #[tokio::test]
    async fn get_todo_returns_full_shape_with_checklist_and_tags() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let full = get_todo(&pool, "todo-1".to_string()).await.unwrap().unwrap();
        assert_eq!(full.summary.title, "Buy milk");
        assert_eq!(full.checklist.len(), 3);
        let titles: Vec<_> = full.checklist.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(titles, vec!["Walk to shop", "Buy whole milk", "Pay with card"]);
        assert!(!full.is_repeating_template);
    }

    #[tokio::test]
    async fn get_todo_returns_none_for_missing_id() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let res = get_todo(&pool, "does-not-exist".to_string()).await.unwrap();
        assert!(res.is_none());
    }
}
