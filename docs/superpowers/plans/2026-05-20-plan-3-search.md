# things-mcp Plan 3 — `things_search` (LIKE-backed) + FTS5 capability detection

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship `things_search` — one MCP tool that combines text search (title + notes) with structured filters (tags, area, project, status, deadline range, scheduled range). The text path uses SQL `LIKE` against the existing `TMTask` columns. As a foundation for future plans, also ship FTS5 capability detection: scan `sqlite_master` at startup, log whether Things' FTS5 indices are present, and stash the resolved `Option<FtsCapability>` on `AppState` so a later plan can wire it in.

**Architecture:** Mirrors the Plan-2 list-tool shape — a query in `core/reader/queries.rs`, an args struct + adapter in a new `tools/search.rs`, and a method on `ThingsServer`. The search query builds its WHERE clause and parameter vector dynamically (each filter contributes 0–2 SQL fragments + bound params). FTS5 detection lives in a new `core/reader/fts.rs` module; it inspects `sqlite_master` for virtual tables `USING fts5` and surfaces the resolved table name + column list. **Plan 3 does not consume that capability for queries** — Things' actual FTS5 schema needs to be inspected against a live install before we can confidently wire it in. That activation step is deferred to a follow-up sub-plan once we have a verified mapping.

**Tech Stack:** Same as Plans 1–2 — no new crate dependencies.

**Spec:** `docs/superpowers/specs/2026-05-20-things-mcp-server-design.md` §4 (Tool surface, last row of the read-tools table).

**Predecessor:** `docs/superpowers/plans/2026-05-20-plan-2-remaining-read-tools.md` — `things_list_inbox` through `things_list_by_tag` already shipped; `SUMMARY_COLS`, `row_to_summary`, `attach_tags`, `fetch_tags_for_tasks`, `decode_things_date`, `pack_things_date`, `parse_iso_date`, `ymd_to_unix_utc`, and the in-code fixture are all in place.

**Scope notes:**
- **FTS5 query path is intentionally not implemented in Plan 3.** Things' production FTS5 schema (table name, join column to `TMTask`, indexed column names) is not documented externally and we haven't inspected a live install. Plan 3 ships detection + startup logging only; a follow-up plan (likely Plan 3.5 or merged into Plan 10's manual-E2E runbook) activates the FTS5 path once verified.
- **Search applies to to-dos only.** `type = 0`, mirroring every Plan-2 list tool. Project search is out of scope until a separate plan asks for it.
- **Tag filter is OR-semantic.** Items matching ANY of the named tags are returned. AND-semantic ("items carrying ALL named tags") is deferred.
- **Text match is a phrase, not a boolean expression.** `LIKE '%query%'` against `title` and `notes`. No `*`, no quoting, no `OR`/`AND` operators. The literal query string flows through.
- **Date bounds are inclusive.** `due_before=2026-12-31` includes the 2026-12-31 row.

**Follow-on plans (unchanged from Plan 1):**
- Plan 3.5 (or merged elsewhere): activate FTS5 query path once verified against a live Things DB
- Plan 4: writer infrastructure (JSON URL builder, dry-run, `open -g`, post-write poll)
- Plan 5: write tools
- Plan 6: AppleScript wrapper + tag admin
- Plan 7: recurrence (experimental)
- Plan 8: HTTP transport + OAuth 2.1 + Tailscale Funnel
- Plan 9: setup / status / show-credentials subcommands + launchd
- Plan 10: docs polish + manual E2E runbook

---

### Task 1: FTS5 capability detection (`core/reader/fts.rs`)

A small read-only probe of `sqlite_master` for virtual tables created with `USING fts5`. Returns the resolved table name and its column list. Tested in isolation — no need for the full fixture.

**Files:**
- Create: `crates/things-mcp/src/core/reader/fts.rs`
- Modify: `crates/things-mcp/src/core/reader/mod.rs`

- [ ] **Step 1: Write the failing tests**

`crates/things-mcp/src/core/reader/fts.rs`:

```rust
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
```

- [ ] **Step 2: Update `core/reader/mod.rs`**

```rust
//! Read path: SQLite connection pool, schema probe, and typed query helpers.

pub mod dates;
pub mod fixture;
pub mod fts;
pub mod pool;
pub mod queries;
pub mod schema;
```

- [ ] **Step 3: Run the tests**

Run: `cargo test --lib core::reader::fts`
Expected: 4 passed.

- [ ] **Step 4: Confirm the full suite is still green**

Run: `cargo test`
Expected: 51 + 4 = 55 tests pass (49 existing lib + 4 new lib + 2 integration).

- [ ] **Step 5: Commit**

```bash
git add crates/things-mcp/src/core/reader
git commit -m "core/reader/fts: FTS5 capability detection via sqlite_master scan"
```

---

### Task 2: AppState wiring — resolve FTS capability at startup + log it

Resolve FTS capability once at startup (after the schema probe, before the reader pool is handed out), surface it on `AppState.fts`, and log the outcome at `INFO`.

**Files:**
- Modify: `crates/things-mcp/src/state.rs`

- [ ] **Step 1: Update `AppState`**

Open `crates/things-mcp/src/state.rs`. Add a field and a startup probe.

Update the imports at the top:

```rust
use crate::core::{
    backup,
    config::{self, Config},
    reader::{fts::{self, FtsCapability}, pool::ReaderPool, schema},
};
```

Add `fts` to the `AppState` struct:

```rust
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db_path: PathBuf,
    pub pool: ReaderPool,
    pub test_db_mode: bool,
    pub allow_writes_on_test_db: bool,
    pub fts: Option<FtsCapability>,
}
```

In `AppState::build`, after `let pool = ReaderPool::new(...)` and before constructing `Self`, run the probe:

```rust
let fts = pool
    .with_conn(move |c| fts::detect(c).map_err(|e| e.into()))
    .await
    .unwrap_or(None);
match &fts {
    Some(cap) => tracing::info!(
        "FTS5 capability: detected (table={}, columns={:?})",
        cap.table,
        cap.columns
    ),
    None => tracing::info!("FTS5 capability: not detected; search uses LIKE fallback"),
}
```

Then include the new field in the struct literal:

```rust
Ok(Self {
    config: Arc::new(cfg),
    db_path,
    pool,
    test_db_mode,
    allow_writes_on_test_db: opts.allow_writes_on_test_db,
    fts,
})
```

Note: `pool.with_conn` returns `Result<T, ThingsError>`. Using `unwrap_or(None)` keeps startup tolerant — a transient detection failure should not block the server. The matching `None` arm produces the same log line as a clean "no FTS5".

- [ ] **Step 2: Build to confirm types line up**

Run: `cargo build`
Expected: clean. No new tests added in this task — Tasks 3 and 5 will exercise the AppState shape end-to-end.

- [ ] **Step 3: Confirm the full suite still passes**

Run: `cargo test`
Expected: 55 tests pass (49 + 4 + 2; AppState change is shape-only and the existing integration tests construct an `AppState` against a non-FTS fixture, which will yield `fts: None`).

- [ ] **Step 4: Commit**

```bash
git add crates/things-mcp/src/state.rs
git commit -m "state: resolve Option<FtsCapability> at startup; log outcome"
```

---

### Task 3: `search` query (LIKE text + structured filters)

The most involved query in Plan 3. Builds a SQL string and parameter vector dynamically based on which filters the caller supplied. Reuses the `SUMMARY_COLS` / `row_to_summary` / `attach_tags` infrastructure from Plan 2.

**Files:**
- Modify: `crates/things-mcp/src/core/reader/queries.rs`

- [ ] **Step 1: Write the failing tests**

Append to the `#[cfg(test)] mod tests` block in `crates/things-mcp/src/core/reader/queries.rs`:

```rust
    #[tokio::test]
    async fn search_text_only_matches_title_and_notes() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let rows = search(
            &pool,
            SearchParams {
                query: Some("milk".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let titles: Vec<_> = rows.iter().map(|r| r.title.as_str()).collect();
        assert!(titles.contains(&"Buy milk"));
        // Status defaults to Open; the completed inbox row is excluded.
        assert!(!titles.contains(&"Pay tax bill"));
    }

    #[tokio::test]
    async fn search_text_search_matches_notes_too() {
        // The fixture's proj-1 has notes "Track what to read next" — projects
        // are not in scope for to-do search (type=0), so the text match
        // should NOT pick them up.
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let rows = search(
            &pool,
            SearchParams {
                query: Some("Track what to read".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert!(rows.is_empty(), "projects must not appear in to-do search");
    }

    #[tokio::test]
    async fn search_tag_filter_or_semantics() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let rows = search(
            &pool,
            SearchParams {
                tags: vec!["Errand".to_string(), "Deep work".to_string()],
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let titles: Vec<_> = rows.iter().map(|r| r.title.as_str()).collect();
        // todo-2 carries 'Errand'; todo-someday carries 'Deep work'.
        assert!(titles.contains(&"Call the dentist"));
        assert!(titles.contains(&"Read research papers"));
    }

    #[tokio::test]
    async fn search_area_filter_includes_project_indirection() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let rows = search(
            &pool,
            SearchParams {
                area_id: Some("area-1".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let titles: Vec<_> = rows.iter().map(|r| r.title.as_str()).collect();
        // todo-4 sits under proj-1 (area-1) — picked up via project indirection.
        assert!(titles.contains(&"Read RFC 9457"));
        // todo-upcoming-dl has area=area-1 directly.
        assert!(titles.contains(&"Upcoming deadlined item"));
    }

    #[tokio::test]
    async fn search_project_filter() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let rows = search(
            &pool,
            SearchParams {
                project_id: Some("proj-1".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let titles: Vec<_> = rows.iter().map(|r| r.title.as_str()).collect();
        assert!(titles.contains(&"Read RFC 9457"));
        // todo-today is also in proj-1 (status=Open).
        assert!(titles.contains(&"Today scheduled item"));
    }

    #[tokio::test]
    async fn search_status_done_includes_logbook() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let rows = search(
            &pool,
            SearchParams {
                status: ProjectStatusFilter::Done,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let titles: Vec<_> = rows.iter().map(|r| r.title.as_str()).collect();
        assert!(titles.contains(&"Old completed"));
        assert!(titles.contains(&"Old canceled"));
        assert!(titles.contains(&"Pay tax bill"));
    }

    #[tokio::test]
    async fn search_deadline_range_filter() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let rows = search(
            &pool,
            SearchParams {
                due_after: Some("2050-01-01".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let titles: Vec<_> = rows.iter().map(|r| r.title.as_str()).collect();
        assert_eq!(titles, vec!["Upcoming deadlined item"]);
    }

    #[tokio::test]
    async fn search_scheduled_range_filter() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let rows = search(
            &pool,
            SearchParams {
                scheduled_before: Some("2050-01-01".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let titles: Vec<_> = rows.iter().map(|r| r.title.as_str()).collect();
        // Scheduled to 2020-01-01 — well before 2050-01-01.
        assert!(titles.contains(&"Today scheduled item"));
        // Scheduled to 2099-12-31 — after the upper bound.
        assert!(!titles.contains(&"Upcoming scheduled item"));
    }

    #[tokio::test]
    async fn search_combined_filters_intersect() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let rows = search(
            &pool,
            SearchParams {
                query: Some("Read".to_string()),
                area_id: Some("area-1".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let titles: Vec<_> = rows.iter().map(|r| r.title.as_str()).collect();
        // Both text-match ("Read") and area-1 match.
        assert!(titles.contains(&"Read RFC 9457"));
        // "Read research papers" is in area-2 — excluded by area filter.
        assert!(!titles.contains(&"Read research papers"));
    }
```

- [ ] **Step 2: Run the new tests to verify they fail**

Run: `cargo test --lib core::reader::queries::tests::search_`
Expected: every test fails with "cannot find function `search`" or "cannot find type `SearchParams`".

- [ ] **Step 3: Add the query**

Append to `crates/things-mcp/src/core/reader/queries.rs`:

```rust
/// Filter inputs to `search`. Each Option / Vec field is OFF when empty/None,
/// matching the spec's "all filters are optional" contract.
#[derive(Default)]
pub struct SearchParams {
    /// Free-text query (LIKE-matched against `title` and `notes`). Optional.
    pub query: Option<String>,
    /// Tag titles or UUIDs. OR-semantic — an item with any listed tag matches.
    pub tags: Vec<String>,
    pub area_id: Option<String>,
    pub project_id: Option<String>,
    pub status: ProjectStatusFilter,
    /// ISO `YYYY-MM-DD`. Inclusive upper bound on `deadline`.
    pub due_before: Option<String>,
    /// ISO `YYYY-MM-DD`. Inclusive lower bound on `deadline`.
    pub due_after: Option<String>,
    /// ISO `YYYY-MM-DD`. Inclusive upper bound on `startDate`.
    pub scheduled_before: Option<String>,
    /// ISO `YYYY-MM-DD`. Inclusive lower bound on `startDate`.
    pub scheduled_after: Option<String>,
    /// Cap on returned rows. Caller supplies; defaults applied at the tool layer.
    pub limit: u32,
}

pub async fn search(
    pool: &ReaderPool,
    params: SearchParams,
) -> Result<Vec<TodoSummary>, ThingsError> {
    use crate::core::reader::dates::{pack_things_date, parse_iso_date};
    use rusqlite::types::Value;

    let mut clauses: Vec<String> = Vec::new();
    let mut binds: Vec<Value> = Vec::new();

    // Status filter — default Open. ProjectStatusFilter is reused (Plan 2)
    // because the enum values map cleanly: Open=0, Done=2|3, All=no filter.
    match params.status {
        ProjectStatusFilter::Open => clauses.push("t.status = 0".to_string()),
        ProjectStatusFilter::Done => clauses.push("t.status IN (2, 3)".to_string()),
        ProjectStatusFilter::All => {}
    }

    // Text filter — LIKE on title + notes.
    if let Some(q) = params.query.as_ref().filter(|s| !s.is_empty()) {
        let pat = format!("%{}%", q);
        clauses.push("(t.title LIKE ? OR t.notes LIKE ?)".to_string());
        binds.push(Value::Text(pat.clone()));
        binds.push(Value::Text(pat));
    }

    // Tag filter — OR-semantic. Inlined EXISTS so the main row scan stays simple.
    if !params.tags.is_empty() {
        let tag_placeholders = (0..params.tags.len() * 2)
            .map(|i| if i % 2 == 0 { "g.title = ?" } else { "g.uuid = ?" })
            .collect::<Vec<_>>()
            .chunks(2)
            .map(|pair| format!("({} OR {})", pair[0], pair[1]))
            .collect::<Vec<_>>()
            .join(" OR ");
        clauses.push(format!(
            "EXISTS (SELECT 1 FROM TMTaskTag tt \
              JOIN TMTag g ON g.uuid = tt.tags \
              WHERE tt.tasks = t.uuid AND ({tag_placeholders}))"
        ));
        for tag in &params.tags {
            binds.push(Value::Text(tag.clone()));
            binds.push(Value::Text(tag.clone()));
        }
    }

    // Area filter — direct OR via project.
    if let Some(area) = params.area_id.as_ref() {
        clauses.push("(t.area = ? OR p.area = ?)".to_string());
        binds.push(Value::Text(area.clone()));
        binds.push(Value::Text(area.clone()));
    }

    // Project filter.
    if let Some(project) = params.project_id.as_ref() {
        clauses.push("t.project = ?".to_string());
        binds.push(Value::Text(project.clone()));
    }

    // Deadline range — packed-int comparison.
    if let Some(iso) = params.due_after.as_ref() {
        let packed = parse_iso_date(iso)
            .map(|(y, m, d)| pack_things_date(y, m, d))
            .ok_or_else(|| ThingsError::InvalidInput {
                field: "due_after".into(),
                reason: format!("expected YYYY-MM-DD, got {iso:?}"),
            })?;
        clauses.push("(t.deadline > 0 AND t.deadline >= ?)".to_string());
        binds.push(Value::Integer(packed));
    }
    if let Some(iso) = params.due_before.as_ref() {
        let packed = parse_iso_date(iso)
            .map(|(y, m, d)| pack_things_date(y, m, d))
            .ok_or_else(|| ThingsError::InvalidInput {
                field: "due_before".into(),
                reason: format!("expected YYYY-MM-DD, got {iso:?}"),
            })?;
        clauses.push("(t.deadline > 0 AND t.deadline <= ?)".to_string());
        binds.push(Value::Integer(packed));
    }

    // Scheduled range — packed-int comparison.
    if let Some(iso) = params.scheduled_after.as_ref() {
        let packed = parse_iso_date(iso)
            .map(|(y, m, d)| pack_things_date(y, m, d))
            .ok_or_else(|| ThingsError::InvalidInput {
                field: "scheduled_after".into(),
                reason: format!("expected YYYY-MM-DD, got {iso:?}"),
            })?;
        clauses.push("(t.startDate > 0 AND t.startDate >= ?)".to_string());
        binds.push(Value::Integer(packed));
    }
    if let Some(iso) = params.scheduled_before.as_ref() {
        let packed = parse_iso_date(iso)
            .map(|(y, m, d)| pack_things_date(y, m, d))
            .ok_or_else(|| ThingsError::InvalidInput {
                field: "scheduled_before".into(),
                reason: format!("expected YYYY-MM-DD, got {iso:?}"),
            })?;
        clauses.push("(t.startDate > 0 AND t.startDate <= ?)".to_string());
        binds.push(Value::Integer(packed));
    }

    let extra = if clauses.is_empty() {
        String::new()
    } else {
        format!(" AND {}", clauses.join(" AND "))
    };
    let sql = format!(
        r#"
        SELECT {SUMMARY_COLS}
        FROM TMTask AS t
        LEFT JOIN TMTask AS p
               ON p.uuid = t.project AND p.type = 1
        WHERE t.trashed = 0
          AND t.type = 0
          {extra}
        ORDER BY t.creationDate DESC
        LIMIT ?
        "#,
    );
    binds.push(Value::Integer(params.limit as i64));

    let rows = pool
        .with_conn(move |c| -> rusqlite::Result<Vec<TodoSummary>> {
            let mut stmt = c.prepare_cached(&sql)?;
            let iter = stmt.query_map(
                rusqlite::params_from_iter(binds.iter()),
                row_to_summary,
            )?;
            iter.collect()
        })
        .await?;
    attach_tags(pool, rows).await
}
```

A couple of notes for the implementer:

1. **Default limit.** `SearchParams::default()` produces `limit: 0`. The query then binds `LIMIT 0` which returns nothing — that's correct in the unit-test path because **every test explicitly supplies a limit via `..Default::default()` after setting individual fields**. Wait — actually the tests use `..Default::default()` to fill in unset fields, so `limit` does land as 0 in tests that don't override it. Override the default by inserting `limit: 200` into every test's `SearchParams { ..., limit: 200 }`, OR adjust the implementation. We adjust the implementation: when `params.limit == 0`, bind a sane internal cap (`i64::MAX`) so unit tests work without ceremony. The MCP-layer adapter (Task 4) always supplies a non-zero limit.

   Add at the top of `search`, just after defining `binds`:

   ```rust
   let effective_limit: i64 = if params.limit == 0 {
       i64::MAX
   } else {
       params.limit as i64
   };
   ```

   And then replace `binds.push(Value::Integer(params.limit as i64));` with `binds.push(Value::Integer(effective_limit));`.

2. **`rusqlite::types::Value` is already in scope?** It's not exported from the crate root by default. Use `use rusqlite::types::Value;` inside the function (as shown), not at the top of the module — keeps the public API of `queries.rs` unchanged.

- [ ] **Step 4: Run the search tests**

Run: `cargo test --lib core::reader::queries::tests::search_`
Expected: 9 passed (the 9 tests added in Step 1).

- [ ] **Step 5: Run the whole `queries` test set to confirm no regressions**

Run: `cargo test --lib core::reader::queries`
Expected: 33 + 9 = 42 passed (existing Plan-2 queries tests + 9 new search tests).

- [ ] **Step 6: Commit**

```bash
git add crates/things-mcp/src/core/reader/queries.rs
git commit -m "core/reader/queries: search with LIKE + structured filters"
```

---

### Task 4: `things_search` MCP tool (`tools/search.rs` + server.rs)

**Files:**
- Create: `crates/things-mcp/src/tools/search.rs`
- Modify: `crates/things-mcp/src/tools/mod.rs`
- Modify: `crates/things-mcp/src/server.rs`

- [ ] **Step 1: Create `tools/search.rs`**

`crates/things-mcp/src/tools/search.rs`:

```rust
//! MCP tool: free-text + structured search over to-dos.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::core::reader::queries::{search, ProjectStatusFilter, SearchParams};
use crate::core::types::TodoSummary;
use crate::state::AppState;
use crate::tools::lists::ProjectStatusArg;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct SearchArgs {
    /// Free-text query, matched against the to-do's title and notes
    /// (case-insensitive substring; no boolean / wildcard syntax).
    #[serde(default)]
    pub query: Option<String>,
    /// Tag titles or UUIDs to match. OR-semantic — an item with any listed tag is returned.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Restrict to to-dos in a specific area (directly or via project). Optional.
    #[serde(default)]
    pub area_id: Option<String>,
    /// Restrict to to-dos in a specific project. Optional.
    #[serde(default)]
    pub project_id: Option<String>,
    /// `open` (default), `done`, or `all`.
    #[serde(default)]
    pub status: Option<ProjectStatusArg>,
    /// ISO `YYYY-MM-DD`. Inclusive upper bound on `deadline`. Optional.
    #[serde(default)]
    pub due_before: Option<String>,
    /// ISO `YYYY-MM-DD`. Inclusive lower bound on `deadline`. Optional.
    #[serde(default)]
    pub due_after: Option<String>,
    /// ISO `YYYY-MM-DD`. Inclusive upper bound on `startDate`. Optional.
    #[serde(default)]
    pub scheduled_before: Option<String>,
    /// ISO `YYYY-MM-DD`. Inclusive lower bound on `startDate`. Optional.
    #[serde(default)]
    pub scheduled_after: Option<String>,
    /// Cap on returned rows. Defaults to 50.
    #[serde(default)]
    pub limit: Option<u32>,
}

pub async fn things_search(
    state: AppState,
    args: SearchArgs,
) -> anyhow::Result<Vec<TodoSummary>> {
    let params = SearchParams {
        query: args.query,
        tags: args.tags,
        area_id: args.area_id,
        project_id: args.project_id,
        status: args.status.unwrap_or_default().into(),
        due_before: args.due_before,
        due_after: args.due_after,
        scheduled_before: args.scheduled_before,
        scheduled_after: args.scheduled_after,
        limit: args.limit.unwrap_or(50),
    };
    let rows = search(&state.pool, params).await?;
    Ok(rows)
}
```

- [ ] **Step 2: Register the module**

`crates/things-mcp/src/tools/mod.rs`:

```rust
pub mod lists;
pub mod projects;
pub mod search;
pub mod todos;
```

- [ ] **Step 3: Register the tool method on `ThingsServer`**

In `crates/things-mcp/src/server.rs`, add a use line for `tools::search` and a method inside the `#[tool_router] impl ThingsServer { ... }` block:

```rust
use crate::tools::search::{things_search, SearchArgs};

    #[tool(
        name = "things_search",
        description = "Search to-dos by free text (title + notes) and structured filters (tags, area, project, status, deadline range, scheduled range). Read-only.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn tool_search(
        &self,
        Parameters(args): Parameters<SearchArgs>,
    ) -> Result<Json<Vec<TodoSummary>>, McpError> {
        let rows = things_search(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(rows))
    }
```

- [ ] **Step 4: Build**

Run: `cargo build`
Expected: clean. No new direct tests (Task 5 supplies the integration test).

- [ ] **Step 5: Full test sweep**

Run: `cargo test`
Expected: 64 tests (60 lib including the 9 new search tests; 2 integration; plus 4 new fts and possibly more).

(Actual count: confirm against `cargo test` summary. The previous Plan-2 end count was 51; Task 1 added 4 fts tests, Task 3 added 9 search tests, Task 2 added 0 tests → expected `51 + 4 + 9 = 64` going into this step.)

- [ ] **Step 6: Commit**

```bash
git add crates/things-mcp/src
git commit -m "tools/search: things_search with text + structured filters"
```

---

### Task 5: End-to-end integration test

**Files:**
- Create: `crates/things-mcp/tests/end_to_end_search.rs`

- [ ] **Step 1: Write the test**

`crates/things-mcp/tests/end_to_end_search.rs`:

```rust
//! End-to-end exercise of `things_search` through the same surface the MCP
//! handler calls. Mirrors `end_to_end_plan_2.rs` — build a fixture DB, build
//! AppState pointed at it, and run the tool function against several
//! filter combinations.

use things_mcp::core::reader::fixture::build_fixture;
use things_mcp::state::{AppState, AppStateOptions};
use things_mcp::tools::lists::ProjectStatusArg;
use things_mcp::tools::search::{things_search, SearchArgs};

async fn build_state() -> AppState {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("things.sqlite");
    build_fixture(&db).unwrap();
    let state = AppState::build(AppStateOptions {
        env_db_path: Some(db.clone()),
        home_dir: tmp.path().to_path_buf(),
        config_path: tmp.path().join("config.toml"),
        allow_writes_on_test_db: false,
    })
    .await
    .unwrap();
    std::mem::forget(tmp);
    state
}

#[tokio::test]
async fn search_handles_text_and_structured_filters() {
    let state = build_state().await;

    // Text-only: open inbox + open anytime to-dos matching "milk".
    let by_text = things_search(
        state.clone(),
        SearchArgs {
            query: Some("milk".to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(by_text.iter().any(|t| t.title == "Buy milk"));

    // Status=All + completed text.
    let with_completed = things_search(
        state.clone(),
        SearchArgs {
            query: Some("tax".to_string()),
            status: Some(ProjectStatusArg::All),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(with_completed.iter().any(|t| t.title == "Pay tax bill"));

    // Tag filter (OR-semantic).
    let by_tag = things_search(
        state.clone(),
        SearchArgs {
            tags: vec!["Errand".to_string()],
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(by_tag.iter().any(|t| t.title == "Call the dentist"));

    // Area filter — todo-4 is in proj-1 (area-1).
    let by_area = things_search(
        state.clone(),
        SearchArgs {
            area_id: Some("area-1".to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let titles: Vec<_> = by_area.iter().map(|t| t.title.as_str()).collect();
    assert!(titles.contains(&"Read RFC 9457"));

    // Project filter.
    let by_project = things_search(
        state.clone(),
        SearchArgs {
            project_id: Some("proj-1".to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let titles: Vec<_> = by_project.iter().map(|t| t.title.as_str()).collect();
    assert!(titles.contains(&"Today scheduled item"));
    assert!(titles.contains(&"Read RFC 9457"));

    // Deadline range — only the 2099-dated row.
    let by_due = things_search(
        state.clone(),
        SearchArgs {
            due_after: Some("2050-01-01".to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let titles: Vec<_> = by_due.iter().map(|t| t.title.as_str()).collect();
    assert_eq!(titles, vec!["Upcoming deadlined item"]);

    // Scheduled range — only the 2020-dated row.
    let by_sched = things_search(
        state.clone(),
        SearchArgs {
            scheduled_before: Some("2050-01-01".to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let titles: Vec<_> = by_sched.iter().map(|t| t.title.as_str()).collect();
    assert!(titles.contains(&"Today scheduled item"));
    assert!(!titles.contains(&"Upcoming scheduled item"));

    // Combined text + area.
    let combined = things_search(
        state.clone(),
        SearchArgs {
            query: Some("Read".to_string()),
            area_id: Some("area-1".to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let titles: Vec<_> = combined.iter().map(|t| t.title.as_str()).collect();
    assert!(titles.contains(&"Read RFC 9457"));
    assert!(!titles.contains(&"Read research papers"));
}
```

- [ ] **Step 2: Run the integration test**

Run: `cargo test --test end_to_end_search`
Expected: 1 passed.

- [ ] **Step 3: Run the whole suite**

Run: `cargo test`
Expected: all tests pass — the Plan-2 baseline (51) plus 4 fts unit tests plus 9 search unit tests plus 1 new integration test = 65 total.

- [ ] **Step 4: Commit**

```bash
git add crates/things-mcp/tests/end_to_end_search.rs
git commit -m "tests: end-to-end exercise of things_search"
```

---

### Task 6: Plan-3 wrap-up

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Update README status**

Open `/Users/rjl/Code/github/things-mcp-server/README.md` and replace the status line set by Plan 2 with:

```markdown
**Status:** Plan 3 — read surface complete (`inbox`/`today`/`upcoming`/`anytime`/`someday`/`logbook`/`trash`/`areas`/`projects`/`tags`/`get_todo`/`get_project`/`list_by_tag`/`search`) over stdio. FTS5 capability is detected at startup; the search query currently uses `LIKE` against `title` and `notes` (FTS5 query path activates in a follow-on once verified against a live Things DB). See `docs/superpowers/plans/` for the active plan and follow-ons.
```

- [ ] **Step 2: Run the full suite + release build**

Run: `cargo test && cargo build --release`
Expected: all tests pass; release build clean.

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: README — plan 3 search complete (LIKE backend + FTS5 detection)"
```

- [ ] **Step 4: Inspect history**

Run: `git log --oneline | head -10`
Expected: 6 new commits on top of the Plan-2 history (one per task in this plan).

---

## Self-review checklist (for the executor)

- [ ] `things_search` is registered on `ThingsServer` with the four MCP annotations.
- [ ] Every filter named in the spec (`query`, `tags`, `area_id`, `project_id`, `status`, `due_before`, `due_after`, `scheduled_before`, `scheduled_after`, `limit`) is exposed on `SearchArgs`.
- [ ] `SearchParams::default().limit == 0` is internally rewritten to `i64::MAX` so default-constructed unit tests work; the MCP-layer adapter always supplies a real limit.
- [ ] `AppState.fts: Option<FtsCapability>` is set at startup; a non-FTS DB yields `None` cleanly and a `tracing::info!` line is emitted on both branches.
- [ ] FTS5 capability is detected via `sqlite_master` scan; `core::reader::fts::detect` is fully tested in isolation with in-memory SQLite (no fixture required).
- [ ] No external crate added to `Cargo.toml`; no `chrono`, no full-text crate, nothing new.
- [ ] Tag filter is OR-semantic; tests assert that.
- [ ] Area filter picks up to-dos belonging to a project in that area (the `LEFT JOIN TMTask AS p ON p.uuid = t.project AND p.type = 1` clause).
- [ ] Deadline / scheduled bound parameters use packed-int comparison via `pack_things_date`; invalid ISO yields `ThingsError::InvalidInput { field, reason }`.
- [ ] Every commit message starts with a module prefix (`core/reader/fts`, `state`, `core/reader/queries`, `tools/search`, `tests`, `docs`).

When all green, the natural next step is **Plan 4** (writer infrastructure — JSON URL builder, dry-run, `open -g`, post-write SQLite poll). FTS5 activation can be a small follow-on (Plan 3.5) once a live Things DB has been inspected.
