# things-mcp Plan 2 — Remaining read tools

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Round out the read surface — `things_list_today`, `things_list_upcoming`, `things_list_anytime`, `things_list_someday`, `things_list_logbook`, `things_list_trash`, `things_list_areas`, `things_list_projects`, `things_list_tags` (with hierarchy), `things_get_todo`, `things_get_project`, `things_list_by_tag` (with recursion) — all against the in-code fixture builder, each verified by unit tests and one end-to-end integration test.

**Architecture:** Each tool follows the Plan 1 pattern: a typed query in `core/reader/queries.rs` (parameterised, `prepare_cached`), a thin args struct + adapter in `tools/{lists,todos,projects}.rs`, and a method in `server::ThingsServer`'s `#[tool_router]` block annotated with MCP hints. A new `core/reader/dates.rs` module decodes Things' bit-packed date columns (`startDate`, `deadline`), the existing `core::reader::fixture::build_fixture` is extended in-place (no new binary test fixtures), and the schema probe gains the new tables it depends on.

**Tech Stack:** Same as Plan 1 — `rmcp 1.7`, `rusqlite 0.39`, `tokio 1`, `schemars 1`. No new crate dependencies.

**Spec:** `docs/superpowers/specs/2026-05-20-things-mcp-server-design.md` §4 (Tool surface, Read tools).

**Predecessor:** `docs/superpowers/plans/2026-05-20-foundation-and-stdio-mcp.md` (foundation + `things_list_inbox`).

**Notes on Things internals carried into this plan:**
- **Date columns.** `TMTask.startDate` and `TMTask.deadline` are bit-packed integers in the form `YYYYYYYYYYY MMMM DDDDD 0000000` (binary, MSB-first): 11 bits of year, 4 bits of month, 5 bits of day, 7 bits of padding. `0` means "no date". A small `decode_things_date(i64) -> Option<String>` lives in `core/reader/dates.rs`; an inverse `pack_things_date(y, m, d) -> i64` lets queries compare against today / user-supplied bounds without rounding through ISO strings.
- **Tag hierarchy.** `TMTag.parent` carries the parent tag's `uuid`. `things_list_tags` returns a flat `Vec<Tag>` whose `parent_id` field lets callers rebuild the tree; `things_list_by_tag` traverses it server-side via a `WITH RECURSIVE` CTE when `recurse=true`.
- **Anytime vs Upcoming.** Anytime = `start=1 AND startDate=0`. Today = `start=1 AND 0 < startDate <= today`. Upcoming = `startDate > today` OR `deadline > today` (an item with a deadline but no startDate appears in both Anytime and Upcoming, mirroring Things' UI).
- **Logbook.** `status IN (2, 3)`, ordered by `stopDate DESC` (a REAL Unix-seconds column).
- **Trash.** `trashed = 1`.
- **`include_evening` for Today.** Deferred. Things' "this evening" sub-list uses an `evening` column we don't yet expose; revisit in a future plan once we exercise it against the live DB.

**Follow-on plans (unchanged from Plan 1):**
- Plan 3: search (`things_search` with FTS5 detection and `LIKE` fallback)
- Plan 4: writer infrastructure (JSON URL builder, dry-run, `open -g`, post-write poll)
- Plan 5: write tools (todos / projects / `assign_tag` / `unassign_tag` / `bulk_json`)
- Plan 6: AppleScript wrapper + tag admin (rename / merge / delete)
- Plan 7: recurrence (experimental)
- Plan 8: HTTP transport + OAuth 2.1 + Tailscale Funnel
- Plan 9: setup / status / show-credentials subcommands + launchd
- Plan 10: docs polish + manual E2E runbook

---

### Task 1: Date helpers (`core/reader/dates.rs`)

**Files:**
- Create: `crates/things-mcp/src/core/reader/dates.rs`
- Modify: `crates/things-mcp/src/core/reader/mod.rs`

- [ ] **Step 1: Write the failing tests**

`crates/things-mcp/src/core/reader/dates.rs`:

```rust
//! Date helpers for the read path.
//!
//! Things stores user-facing dates (`startDate`, `deadline`) as bit-packed
//! integers:
//!
//! ```text
//!   bit  26                              0
//!        YYYYYYYYYYY MMMM DDDDD 0000000
//!        ↑ 11 bits   ↑ 4   ↑ 5   ↑ 7 bits padding
//! ```
//!
//! `0` means "no date" (Things never writes `1970-00-00`). Things stores
//! row-modification timestamps (`creationDate`, `userModificationDate`,
//! `stopDate`) separately as REAL Unix seconds — those go through
//! `unix_to_iso` over in `queries.rs`, not here.

/// Decode a Things packed date into an ISO `YYYY-MM-DD` string.
/// Returns `None` for `0` and for out-of-range / malformed values so a
/// future schema change can't surface garbage to callers.
pub fn decode_things_date(packed: i64) -> Option<String> {
    if packed == 0 {
        return None;
    }
    let year = ((packed >> 16) & 0x7FF) as i32;
    let month = ((packed >> 12) & 0x0F) as u32;
    let day = ((packed >> 7) & 0x1F) as u32;
    if year < 1900 || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

/// Pack a `(year, month, day)` triple back into Things' format. Used by
/// query helpers that need to compare against `today` or user-supplied
/// date bounds without round-tripping through ISO strings.
pub fn pack_things_date(year: i32, month: u32, day: u32) -> i64 {
    ((year as i64) << 16) | ((month as i64) << 12) | ((day as i64) << 7)
}

/// Parse `YYYY-MM-DD` into `(y, m, d)`. Returns `None` on any deviation
/// from the strict 10-character ISO date form so caller validation is
/// straightforward.
pub fn parse_iso_date(iso: &str) -> Option<(i32, u32, u32)> {
    if iso.len() != 10 {
        return None;
    }
    let bytes = iso.as_bytes();
    if bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let y: i32 = iso.get(0..4)?.parse().ok()?;
    let m: u32 = iso.get(5..7)?.parse().ok()?;
    let d: u32 = iso.get(8..10)?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some((y, m, d))
}

/// `(year, month, day)` → Unix epoch seconds (00:00 UTC of that date).
/// Inverse of `core::backup::__test_only_unix_to_ymdhms` rounded to whole
/// days. Used by `things_list_logbook`'s `from`/`to` filters which compare
/// against `stopDate` (REAL Unix seconds).
pub fn ymd_to_unix_utc(year: i32, month: u32, day: u32) -> i64 {
    let mut days: i64 = 0;
    let from_year = 1970;
    if year >= from_year {
        for yi in from_year..year {
            let leap = (yi % 4 == 0 && yi % 100 != 0) || (yi % 400 == 0);
            days += if leap { 366 } else { 365 };
        }
    } else {
        for yi in year..from_year {
            let leap = (yi % 4 == 0 && yi % 100 != 0) || (yi % 400 == 0);
            days -= if leap { 366 } else { 365 };
        }
    }
    let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let months_len: [u32; 12] = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    for mo in 1..month {
        days += months_len[(mo - 1) as usize] as i64;
    }
    days += (day - 1) as i64;
    days * 86_400
}

/// Today's date (UTC), packed for direct comparison against Things'
/// `startDate` / `deadline` columns.
pub fn today_packed_utc() -> i64 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (y, m, d, _, _, _) = crate::core::backup::__test_only_unix_to_ymdhms(secs);
    pack_things_date(y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_zero_is_none() {
        assert_eq!(decode_things_date(0), None);
    }

    #[test]
    fn decode_known_packed_value() {
        // pack(2026, 5, 20)
        let p = pack_things_date(2026, 5, 20);
        assert_eq!(decode_things_date(p), Some("2026-05-20".to_string()));
    }

    #[test]
    fn pack_then_decode_round_trip() {
        for (y, m, d) in [(2000, 1, 1), (2026, 12, 31), (2099, 12, 31)] {
            let p = pack_things_date(y, m, d);
            assert_eq!(
                decode_things_date(p),
                Some(format!("{y:04}-{m:02}-{d:02}"))
            );
        }
    }

    #[test]
    fn decode_rejects_malformed_year() {
        // Year 1800 — below the 1900 sanity cutoff, mark as malformed.
        let p = pack_things_date(1800, 1, 1);
        assert_eq!(decode_things_date(p), None);
    }

    #[test]
    fn parse_iso_date_happy_path() {
        assert_eq!(parse_iso_date("2026-05-20"), Some((2026, 5, 20)));
    }

    #[test]
    fn parse_iso_date_rejects_wrong_separators() {
        assert_eq!(parse_iso_date("2026/05/20"), None);
    }

    #[test]
    fn parse_iso_date_rejects_wrong_length() {
        assert_eq!(parse_iso_date("2026-5-20"), None);
        assert_eq!(parse_iso_date("2026-05-2"), None);
    }

    #[test]
    fn ymd_to_unix_utc_at_epoch() {
        assert_eq!(ymd_to_unix_utc(1970, 1, 1), 0);
        assert_eq!(ymd_to_unix_utc(1970, 1, 2), 86_400);
    }

    #[test]
    fn ymd_to_unix_utc_leap_day() {
        // 2024-02-29 → days = 365*54 + 13 leap days + 31 (jan) + 28 (feb) days
        // We don't hard-code the answer; just check that 2024-03-01 is one day
        // after 2024-02-29.
        let a = ymd_to_unix_utc(2024, 2, 29);
        let b = ymd_to_unix_utc(2024, 3, 1);
        assert_eq!(b - a, 86_400);
    }

    #[test]
    fn today_packed_utc_decodes_to_real_date() {
        let p = today_packed_utc();
        let s = decode_things_date(p).expect("today must decode");
        assert_eq!(s.len(), 10);
        assert_eq!(&s[4..5], "-");
        assert_eq!(&s[7..8], "-");
    }
}
```

- [ ] **Step 2: Update `core/reader/mod.rs`**

```rust
//! Read path: SQLite connection pool, schema probe, and typed query helpers.

pub mod dates;
pub mod fixture;
pub mod pool;
pub mod queries;
pub mod schema;
```

- [ ] **Step 3: Run the tests**

Run: `cargo test --lib core::reader::dates`
Expected: 10 passed.

- [ ] **Step 4: Commit**

```bash
git add crates/things-mcp/src/core/reader
git commit -m "core/reader/dates: bit-packed date decode + ISO helpers"
```

---

### Task 2: `row_to_summary` helper + `list_inbox` refactor

Plan 1 left `TodoSummary.scheduled` and `TodoSummary.deadline` stubbed as `None` because the date decoder didn't exist yet. With Task 1's decoder in place, lift the row-mapping into a single helper and update `list_inbox` to populate both date fields. Every list query from Task 5 onward consumes the same helper.

**Files:**
- Modify: `crates/things-mcp/src/core/reader/queries.rs`

- [ ] **Step 1: Add the helper at the top of `queries.rs`**

After the existing `use` block, before `pub struct ListInboxParams`, add:

```rust
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
```

- [ ] **Step 2: Replace `list_inbox`'s body to use the helper**

In `queries.rs`, replace the existing `list_inbox` body. The new SQL adds `t.startDate, t.deadline` to the projection (same order as `SUMMARY_COLS`) and the row-mapping calls `row_to_summary`:

```rust
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
```

- [ ] **Step 3: Run existing inbox tests to confirm no regression**

Run: `cargo test --lib core::reader::queries`
Expected: 3 passed (`list_inbox_default_excludes_completed`, `list_inbox_with_completed_includes_completed`, `list_inbox_attaches_tags`).

- [ ] **Step 4: Commit**

```bash
git add crates/things-mcp/src/core/reader/queries.rs
git commit -m "core/reader/queries: row_to_summary helper; list_inbox emits scheduled/deadline"
```

---

### Task 3: Extend the fixture builder for Plan-2 coverage

Add the rows and one new table (`TMChecklistItem`) every subsequent task needs. The existing inbox tests must still pass after this change — they only assert counts for `start=0 AND trashed=0` and tags on `todo-2`, both unaffected.

**Files:**
- Modify: `crates/things-mcp/src/core/reader/fixture.rs`

- [ ] **Step 1: Replace `build_fixture` with the extended schema + seed data**

Replace the entire body of `build_fixture` in `crates/things-mcp/src/core/reader/fixture.rs`:

```rust
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
```

- [ ] **Step 2: Update the existing smoke test**

The existing `fixture_has_expected_inbox_rows` test in the same file still passes (it counts `start = 0 AND trashed = 0` which is unchanged at 3). Leave it as-is.

- [ ] **Step 3: Run the fixture's own test plus the inbox tests**

Run: `cargo test --lib core::reader::fixture core::reader::queries`
Expected: 1 + 3 = 4 passed.

- [ ] **Step 4: Commit**

```bash
git add crates/things-mcp/src/core/reader/fixture.rs
git commit -m "core/reader/fixture: extended seed data for plan-2 read tools"
```

---

### Task 4: Extend the schema probe

Schema-probe needs to require the columns the new queries reference and the new `TMChecklistItem` table. Without this update, a real Things install lacking those columns would surface as a confusing query error instead of the structured `SchemaIncompatible` error.

**Files:**
- Modify: `crates/things-mcp/src/core/reader/schema.rs`

- [ ] **Step 1: Update the `REQUIRED` table**

In `crates/things-mcp/src/core/reader/schema.rs` replace the `REQUIRED` constant:

```rust
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
```

- [ ] **Step 2: Run schema-probe tests against the extended fixture**

Run: `cargo test --lib core::reader::schema`
Expected: 2 passed (`probe_passes_on_fixture`, `probe_reports_missing_columns`). The first passes because Task 3's fixture now creates `TMChecklistItem` + the `"index"` columns the probe requires.

- [ ] **Step 3: Commit**

```bash
git add crates/things-mcp/src/core/reader/schema.rs
git commit -m "core/reader/schema: require checklist table + index/todayIndex columns"
```

---

### Task 5: `things_list_today` (query + tool)

**Files:**
- Modify: `crates/things-mcp/src/core/reader/queries.rs`
- Modify: `crates/things-mcp/src/tools/lists.rs`
- Modify: `crates/things-mcp/src/server.rs`

- [ ] **Step 1: Write the failing test**

Append to the `#[cfg(test)] mod tests` block in `crates/things-mcp/src/core/reader/queries.rs`:

```rust
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
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test --lib core::reader::queries::tests::list_today_includes_past_scheduled`
Expected: FAIL with "cannot find function `list_today` in this scope" (or `ListTodayParams`).

- [ ] **Step 3: Add the query**

Add to `queries.rs` (after `list_inbox`):

```rust
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
```

While you're in the file, replace the tail of `list_inbox` (the manual ids/tag_map loop) with `attach_tags(pool, rows).await` so both queries share one path:

```rust
    // (inside list_inbox, replacing the explicit loop)
    attach_tags(pool, rows).await
```

- [ ] **Step 4: Run the new test and the existing inbox tests**

Run: `cargo test --lib core::reader::queries`
Expected: 4 passed (3 inbox + 1 today).

- [ ] **Step 5: Add the MCP tool layer in `tools/lists.rs`**

Append to `crates/things-mcp/src/tools/lists.rs`:

```rust
use crate::core::reader::queries::{list_today, ListTodayParams};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct ListTodayArgs {
    /// Cap on returned rows. Defaults to 200.
    #[serde(default)]
    pub limit: Option<u32>,
}

pub async fn things_list_today(
    state: AppState,
    args: ListTodayArgs,
) -> anyhow::Result<Vec<TodoSummary>> {
    let params = ListTodayParams {
        limit: args.limit.unwrap_or(200),
    };
    let rows = list_today(&state.pool, params).await?;
    Ok(rows)
}
```

- [ ] **Step 6: Register the tool in `server.rs`**

In `crates/things-mcp/src/server.rs`, update the `use` line for `tools::lists` to import the new symbols and add the tool method inside the `#[tool_router] impl ThingsServer { ... }` block. The block should look like:

```rust
use crate::tools::lists::{
    things_list_inbox, things_list_today, ListInboxArgs, ListTodayArgs,
};

// inside #[tool_router] impl ThingsServer { ... }
    #[tool(
        name = "things_list_today",
        description = "Return to-dos scheduled for today (start = Anytime with startDate ≤ today). Read-only.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn tool_list_today(
        &self,
        Parameters(args): Parameters<ListTodayArgs>,
    ) -> Result<Json<Vec<TodoSummary>>, McpError> {
        let rows = things_list_today(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(rows))
    }
```

- [ ] **Step 7: Build and run all tests**

Run: `cargo build && cargo test`
Expected: clean build; all tests pass.

- [ ] **Step 8: Commit**

```bash
git add crates/things-mcp/src
git commit -m "tools/lists: things_list_today (scheduled <= today, anytime bucket)"
```

---

### Task 6: `things_list_upcoming` (query + tool)

**Files:**
- Modify: `crates/things-mcp/src/core/reader/queries.rs`
- Modify: `crates/things-mcp/src/tools/lists.rs`
- Modify: `crates/things-mcp/src/server.rs`

- [ ] **Step 1: Write the failing test**

Append to the `tests` mod in `queries.rs`:

```rust
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
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test --lib core::reader::queries::tests::list_upcoming_returns_future_scheduled_and_deadlined`
Expected: FAIL ("cannot find function `list_upcoming`").

- [ ] **Step 3: Add the query**

Append to `queries.rs`:

```rust
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
```

- [ ] **Step 4: Run the new tests**

Run: `cargo test --lib core::reader::queries::tests::list_upcoming`
Expected: 2 passed.

- [ ] **Step 5: Add the MCP tool layer**

Append to `tools/lists.rs`:

```rust
use crate::core::reader::queries::{list_upcoming, ListUpcomingParams};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct ListUpcomingArgs {
    /// Lower bound (exclusive) as `YYYY-MM-DD`. Defaults to today.
    #[serde(default)]
    pub from: Option<String>,
    /// Upper bound (inclusive) as `YYYY-MM-DD`. If omitted, no upper bound.
    #[serde(default)]
    pub to: Option<String>,
    /// Cap on returned rows. Defaults to 200.
    #[serde(default)]
    pub limit: Option<u32>,
}

pub async fn things_list_upcoming(
    state: AppState,
    args: ListUpcomingArgs,
) -> anyhow::Result<Vec<TodoSummary>> {
    let params = ListUpcomingParams {
        from_iso: args.from,
        to_iso: args.to,
        limit: args.limit.unwrap_or(200),
    };
    let rows = list_upcoming(&state.pool, params).await?;
    Ok(rows)
}
```

- [ ] **Step 6: Register the tool in `server.rs`**

Extend the `use` line and add the tool method in the `#[tool_router]` block (mirror the Task-5 pattern):

```rust
use crate::tools::lists::{
    things_list_inbox, things_list_today, things_list_upcoming,
    ListInboxArgs, ListTodayArgs, ListUpcomingArgs,
};

    #[tool(
        name = "things_list_upcoming",
        description = "Return scheduled or deadlined to-dos in the future. Read-only.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn tool_list_upcoming(
        &self,
        Parameters(args): Parameters<ListUpcomingArgs>,
    ) -> Result<Json<Vec<TodoSummary>>, McpError> {
        let rows = things_list_upcoming(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(rows))
    }
```

- [ ] **Step 7: Build and run tests**

Run: `cargo build && cargo test`
Expected: clean.

- [ ] **Step 8: Commit**

```bash
git add crates/things-mcp/src
git commit -m "tools/lists: things_list_upcoming with from/to ISO bounds"
```

---

### Task 7: `things_list_anytime` (query + tool)

**Files:**
- Modify: `crates/things-mcp/src/core/reader/queries.rs`
- Modify: `crates/things-mcp/src/tools/lists.rs`
- Modify: `crates/things-mcp/src/server.rs`

- [ ] **Step 1: Write the failing test**

Append to the `tests` mod:

```rust
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
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test --lib core::reader::queries::tests::list_anytime`
Expected: FAIL ("cannot find function `list_anytime`").

- [ ] **Step 3: Add the query**

Append to `queries.rs`:

```rust
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
```

- [ ] **Step 4: Run the new tests**

Run: `cargo test --lib core::reader::queries::tests::list_anytime`
Expected: 2 passed.

- [ ] **Step 5: Add the MCP tool layer**

Append to `tools/lists.rs`:

```rust
use crate::core::reader::queries::{list_anytime, ListAnytimeParams};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct ListAnytimeArgs {
    /// Restrict to to-dos belonging to a specific area (directly or via project). Optional.
    #[serde(default)]
    pub area_id: Option<String>,
    /// Cap on returned rows. Defaults to 200.
    #[serde(default)]
    pub limit: Option<u32>,
}

pub async fn things_list_anytime(
    state: AppState,
    args: ListAnytimeArgs,
) -> anyhow::Result<Vec<TodoSummary>> {
    let params = ListAnytimeParams {
        area_id: args.area_id,
        limit: args.limit.unwrap_or(200),
    };
    let rows = list_anytime(&state.pool, params).await?;
    Ok(rows)
}
```

- [ ] **Step 6: Register the tool**

In `server.rs`, extend the `use` and add the method (mirror Task 5):

```rust
use crate::tools::lists::{
    things_list_anytime, things_list_inbox, things_list_today, things_list_upcoming,
    ListAnytimeArgs, ListInboxArgs, ListTodayArgs, ListUpcomingArgs,
};

    #[tool(
        name = "things_list_anytime",
        description = "Return Anytime to-dos (start=Anytime, no scheduled date). Optionally filter by area. Read-only.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn tool_list_anytime(
        &self,
        Parameters(args): Parameters<ListAnytimeArgs>,
    ) -> Result<Json<Vec<TodoSummary>>, McpError> {
        let rows = things_list_anytime(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(rows))
    }
```

- [ ] **Step 7: Build and run tests**

Run: `cargo build && cargo test`
Expected: clean.

- [ ] **Step 8: Commit**

```bash
git add crates/things-mcp/src
git commit -m "tools/lists: things_list_anytime with area filter"
```

---

### Task 8: `things_list_someday` (query + tool)

**Files:**
- Modify: `crates/things-mcp/src/core/reader/queries.rs`
- Modify: `crates/things-mcp/src/tools/lists.rs`
- Modify: `crates/things-mcp/src/server.rs`

- [ ] **Step 1: Write the failing test**

Append to the `tests` mod:

```rust
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
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test --lib core::reader::queries::tests::list_someday`
Expected: FAIL ("cannot find function `list_someday`").

- [ ] **Step 3: Add the query**

Append to `queries.rs`:

```rust
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
```

- [ ] **Step 4: Run the new test**

Run: `cargo test --lib core::reader::queries::tests::list_someday`
Expected: 1 passed.

- [ ] **Step 5: Add the MCP tool layer**

Append to `tools/lists.rs`:

```rust
use crate::core::reader::queries::{list_someday, ListSomedayParams};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct ListSomedayArgs {
    /// Cap on returned rows. Defaults to 200.
    #[serde(default)]
    pub limit: Option<u32>,
}

pub async fn things_list_someday(
    state: AppState,
    args: ListSomedayArgs,
) -> anyhow::Result<Vec<TodoSummary>> {
    let params = ListSomedayParams {
        limit: args.limit.unwrap_or(200),
    };
    let rows = list_someday(&state.pool, params).await?;
    Ok(rows)
}
```

- [ ] **Step 6: Register the tool**

In `server.rs`, extend the `use` line and add the method:

```rust
use crate::tools::lists::{
    things_list_anytime, things_list_inbox, things_list_someday, things_list_today,
    things_list_upcoming, ListAnytimeArgs, ListInboxArgs, ListSomedayArgs, ListTodayArgs,
    ListUpcomingArgs,
};

    #[tool(
        name = "things_list_someday",
        description = "Return Someday to-dos (start = Someday). Read-only.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn tool_list_someday(
        &self,
        Parameters(args): Parameters<ListSomedayArgs>,
    ) -> Result<Json<Vec<TodoSummary>>, McpError> {
        let rows = things_list_someday(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(rows))
    }
```

- [ ] **Step 7: Build and run tests**

Run: `cargo build && cargo test`
Expected: clean.

- [ ] **Step 8: Commit**

```bash
git add crates/things-mcp/src
git commit -m "tools/lists: things_list_someday"
```

---

### Task 9: `things_list_logbook` (query + tool with from/to)

**Files:**
- Modify: `crates/things-mcp/src/core/reader/queries.rs`
- Modify: `crates/things-mcp/src/tools/lists.rs`
- Modify: `crates/things-mcp/src/server.rs`

- [ ] **Step 1: Write the failing test**

Append to the `tests` mod:

```rust
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
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test --lib core::reader::queries::tests::list_logbook`
Expected: FAIL ("cannot find function `list_logbook`").

- [ ] **Step 3: Add the query**

Append to `queries.rs`:

```rust
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
```

- [ ] **Step 4: Run the new tests**

Run: `cargo test --lib core::reader::queries::tests::list_logbook`
Expected: 2 passed.

- [ ] **Step 5: Add the MCP tool layer**

Append to `tools/lists.rs`:

```rust
use crate::core::reader::queries::{list_logbook, ListLogbookParams};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct ListLogbookArgs {
    /// Lower bound on completion date as `YYYY-MM-DD` (inclusive). Optional.
    #[serde(default)]
    pub from: Option<String>,
    /// Upper bound on completion date as `YYYY-MM-DD` (inclusive — end-of-day). Optional.
    #[serde(default)]
    pub to: Option<String>,
    /// Cap on returned rows. Defaults to 100.
    #[serde(default)]
    pub limit: Option<u32>,
}

pub async fn things_list_logbook(
    state: AppState,
    args: ListLogbookArgs,
) -> anyhow::Result<Vec<TodoSummary>> {
    let params = ListLogbookParams {
        from_iso: args.from,
        to_iso: args.to,
        limit: args.limit.unwrap_or(100),
    };
    let rows = list_logbook(&state.pool, params).await?;
    Ok(rows)
}
```

- [ ] **Step 6: Register the tool**

In `server.rs`, extend the `use` and add the method:

```rust
use crate::tools::lists::{
    things_list_anytime, things_list_inbox, things_list_logbook, things_list_someday,
    things_list_today, things_list_upcoming, ListAnytimeArgs, ListInboxArgs,
    ListLogbookArgs, ListSomedayArgs, ListTodayArgs, ListUpcomingArgs,
};

    #[tool(
        name = "things_list_logbook",
        description = "Return completed or canceled to-dos, newest first. Read-only.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn tool_list_logbook(
        &self,
        Parameters(args): Parameters<ListLogbookArgs>,
    ) -> Result<Json<Vec<TodoSummary>>, McpError> {
        let rows = things_list_logbook(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(rows))
    }
```

- [ ] **Step 7: Build and run tests**

Run: `cargo build && cargo test`
Expected: clean.

- [ ] **Step 8: Commit**

```bash
git add crates/things-mcp/src
git commit -m "tools/lists: things_list_logbook with stopDate bounds"
```

---

### Task 10: `things_list_trash` (query + tool)

**Files:**
- Modify: `crates/things-mcp/src/core/reader/queries.rs`
- Modify: `crates/things-mcp/src/tools/lists.rs`
- Modify: `crates/things-mcp/src/server.rs`

- [ ] **Step 1: Write the failing test**

Append to the `tests` mod:

```rust
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
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test --lib core::reader::queries::tests::list_trash`
Expected: FAIL.

- [ ] **Step 3: Add the query**

Append to `queries.rs`:

```rust
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
```

- [ ] **Step 4: Run the new test**

Run: `cargo test --lib core::reader::queries::tests::list_trash`
Expected: 1 passed.

- [ ] **Step 5: Add the MCP tool layer**

Append to `tools/lists.rs`:

```rust
use crate::core::reader::queries::{list_trash, ListTrashParams};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct ListTrashArgs {
    /// Cap on returned rows. Defaults to 100.
    #[serde(default)]
    pub limit: Option<u32>,
}

pub async fn things_list_trash(
    state: AppState,
    args: ListTrashArgs,
) -> anyhow::Result<Vec<TodoSummary>> {
    let params = ListTrashParams {
        limit: args.limit.unwrap_or(100),
    };
    let rows = list_trash(&state.pool, params).await?;
    Ok(rows)
}
```

- [ ] **Step 6: Register the tool**

In `server.rs`, extend the `use` line and add the method:

```rust
use crate::tools::lists::{
    things_list_anytime, things_list_inbox, things_list_logbook, things_list_someday,
    things_list_today, things_list_trash, things_list_upcoming, ListAnytimeArgs,
    ListInboxArgs, ListLogbookArgs, ListSomedayArgs, ListTodayArgs, ListTrashArgs,
    ListUpcomingArgs,
};

    #[tool(
        name = "things_list_trash",
        description = "Return trashed to-dos, newest first. Read-only.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn tool_list_trash(
        &self,
        Parameters(args): Parameters<ListTrashArgs>,
    ) -> Result<Json<Vec<TodoSummary>>, McpError> {
        let rows = things_list_trash(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(rows))
    }
```

- [ ] **Step 7: Build and run tests**

Run: `cargo build && cargo test`
Expected: clean.

- [ ] **Step 8: Commit**

```bash
git add crates/things-mcp/src
git commit -m "tools/lists: things_list_trash"
```

---

### Task 11: `things_list_areas` (query + tool)

**Files:**
- Modify: `crates/things-mcp/src/core/reader/queries.rs`
- Modify: `crates/things-mcp/src/tools/lists.rs`
- Modify: `crates/things-mcp/src/server.rs`

- [ ] **Step 1: Write the failing test**

Append to the `tests` mod:

```rust
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
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test --lib core::reader::queries::tests::list_areas`
Expected: FAIL.

- [ ] **Step 3: Add the query**

Add to the top of `queries.rs`, update the `use` line for `types`:

```rust
use crate::core::types::{Area, StartBucket, TaskStatus, TodoSummary};
```

Append to `queries.rs`:

```rust
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
```

- [ ] **Step 4: Run the new test**

Run: `cargo test --lib core::reader::queries::tests::list_areas`
Expected: 1 passed.

- [ ] **Step 5: Add the MCP tool layer**

Append to `tools/lists.rs`:

```rust
use crate::core::reader::queries::list_areas;
use crate::core::types::Area;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct ListAreasArgs {}

pub async fn things_list_areas(
    state: AppState,
    _args: ListAreasArgs,
) -> anyhow::Result<Vec<Area>> {
    let rows = list_areas(&state.pool).await?;
    Ok(rows)
}
```

- [ ] **Step 6: Register the tool**

In `server.rs`, update imports and add the method:

```rust
use crate::core::types::{Area, TodoSummary};
use crate::tools::lists::{
    things_list_anytime, things_list_areas, things_list_inbox, things_list_logbook,
    things_list_someday, things_list_today, things_list_trash, things_list_upcoming,
    ListAnytimeArgs, ListAreasArgs, ListInboxArgs, ListLogbookArgs, ListSomedayArgs,
    ListTodayArgs, ListTrashArgs, ListUpcomingArgs,
};

    #[tool(
        name = "things_list_areas",
        description = "Return all areas, ordered by display index. Read-only.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn tool_list_areas(
        &self,
        Parameters(args): Parameters<ListAreasArgs>,
    ) -> Result<Json<Vec<Area>>, McpError> {
        let rows = things_list_areas(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(rows))
    }
```

- [ ] **Step 7: Build and run tests**

Run: `cargo build && cargo test`
Expected: clean.

- [ ] **Step 8: Commit**

```bash
git add crates/things-mcp/src
git commit -m "tools/lists: things_list_areas"
```

---

### Task 12: `things_list_projects` (query + tool with status enum)

**Files:**
- Modify: `crates/things-mcp/src/core/reader/queries.rs`
- Modify: `crates/things-mcp/src/tools/lists.rs`
- Modify: `crates/things-mcp/src/server.rs`

- [ ] **Step 1: Write the failing tests**

Append to the `tests` mod:

```rust
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
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test --lib core::reader::queries::tests::list_projects`
Expected: FAIL.

- [ ] **Step 3: Add the query**

Update the `use` line at the top of `queries.rs` to include `Project`:

```rust
use crate::core::types::{Area, Project, StartBucket, TaskStatus, TodoSummary};
```

Append to `queries.rs`:

```rust
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
```

- [ ] **Step 4: Run the new tests**

Run: `cargo test --lib core::reader::queries::tests::list_projects`
Expected: 3 passed.

- [ ] **Step 5: Add the MCP tool layer**

Append to `tools/lists.rs`:

```rust
use crate::core::reader::queries::{list_projects, ListProjectsParams, ProjectStatusFilter};
use crate::core::types::Project;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProjectStatusArg {
    #[default]
    Open,
    Done,
    All,
}

impl From<ProjectStatusArg> for ProjectStatusFilter {
    fn from(a: ProjectStatusArg) -> Self {
        match a {
            ProjectStatusArg::Open => Self::Open,
            ProjectStatusArg::Done => Self::Done,
            ProjectStatusArg::All => Self::All,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct ListProjectsArgs {
    /// Restrict to projects in a given area. Optional.
    #[serde(default)]
    pub area_id: Option<String>,
    /// `open` (default), `done`, or `all`.
    #[serde(default)]
    pub status: Option<ProjectStatusArg>,
}

pub async fn things_list_projects(
    state: AppState,
    args: ListProjectsArgs,
) -> anyhow::Result<Vec<Project>> {
    let params = ListProjectsParams {
        area_id: args.area_id,
        status: args.status.unwrap_or_default().into(),
    };
    let rows = list_projects(&state.pool, params).await?;
    Ok(rows)
}
```

- [ ] **Step 6: Register the tool**

In `server.rs`, extend the `use` and add the method:

```rust
use crate::core::types::{Area, Project, TodoSummary};
use crate::tools::lists::{
    things_list_anytime, things_list_areas, things_list_inbox, things_list_logbook,
    things_list_projects, things_list_someday, things_list_today, things_list_trash,
    things_list_upcoming, ListAnytimeArgs, ListAreasArgs, ListInboxArgs, ListLogbookArgs,
    ListProjectsArgs, ListSomedayArgs, ListTodayArgs, ListTrashArgs, ListUpcomingArgs,
};

    #[tool(
        name = "things_list_projects",
        description = "Return projects, optionally restricted to a single area and/or a status filter (open/done/all). Read-only.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn tool_list_projects(
        &self,
        Parameters(args): Parameters<ListProjectsArgs>,
    ) -> Result<Json<Vec<Project>>, McpError> {
        let rows = things_list_projects(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(rows))
    }
```

- [ ] **Step 7: Build and run tests**

Run: `cargo build && cargo test`
Expected: clean.

- [ ] **Step 8: Commit**

```bash
git add crates/things-mcp/src
git commit -m "tools/lists: things_list_projects with area + status filter"
```

---

### Task 13: `things_list_tags` (query + tool, returns flat list + parent linkage)

**Files:**
- Modify: `crates/things-mcp/src/core/reader/queries.rs`
- Modify: `crates/things-mcp/src/tools/lists.rs`
- Modify: `crates/things-mcp/src/server.rs`

- [ ] **Step 1: Write the failing test**

Append to the `tests` mod:

```rust
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
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test --lib core::reader::queries::tests::list_tags`
Expected: FAIL.

- [ ] **Step 3: Add the query**

Update the `use` line at the top of `queries.rs` to include `Tag`:

```rust
use crate::core::types::{Area, Project, StartBucket, Tag, TaskStatus, TodoSummary};
```

Append to `queries.rs`:

```rust
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
```

- [ ] **Step 4: Run the new test**

Run: `cargo test --lib core::reader::queries::tests::list_tags`
Expected: 1 passed.

- [ ] **Step 5: Add the MCP tool layer**

Append to `tools/lists.rs`:

```rust
use crate::core::reader::queries::list_tags;
use crate::core::types::Tag;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct ListTagsArgs {}

pub async fn things_list_tags(
    state: AppState,
    _args: ListTagsArgs,
) -> anyhow::Result<Vec<Tag>> {
    let rows = list_tags(&state.pool).await?;
    Ok(rows)
}
```

- [ ] **Step 6: Register the tool**

In `server.rs`, extend the `use` and add the method:

```rust
use crate::core::types::{Area, Project, Tag, TodoSummary};
use crate::tools::lists::{
    things_list_anytime, things_list_areas, things_list_inbox, things_list_logbook,
    things_list_projects, things_list_someday, things_list_tags, things_list_today,
    things_list_trash, things_list_upcoming, ListAnytimeArgs, ListAreasArgs,
    ListInboxArgs, ListLogbookArgs, ListProjectsArgs, ListSomedayArgs, ListTagsArgs,
    ListTodayArgs, ListTrashArgs, ListUpcomingArgs,
};

    #[tool(
        name = "things_list_tags",
        description = "Return all tags. Each carries `parent_id` so callers can rebuild the hierarchy. Read-only.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn tool_list_tags(
        &self,
        Parameters(args): Parameters<ListTagsArgs>,
    ) -> Result<Json<Vec<Tag>>, McpError> {
        let rows = things_list_tags(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(rows))
    }
```

- [ ] **Step 7: Build and run tests**

Run: `cargo build && cargo test`
Expected: clean.

- [ ] **Step 8: Commit**

```bash
git add crates/things-mcp/src
git commit -m "tools/lists: things_list_tags with parent_id"
```

---

### Task 14: `things_get_todo` (query + tool in `tools/todos.rs`)

`get_todo` returns a full `TodoFull` with notes, checklist items, and `is_repeating_template`. This is the first read tool that lives outside `tools/lists.rs` — create `tools/todos.rs` and register it under `mod tools`.

**Files:**
- Modify: `crates/things-mcp/src/core/reader/queries.rs`
- Create: `crates/things-mcp/src/tools/todos.rs`
- Modify: `crates/things-mcp/src/tools/mod.rs`
- Modify: `crates/things-mcp/src/server.rs`

- [ ] **Step 1: Write the failing test**

Append to the `tests` mod in `queries.rs`:

```rust
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
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test --lib core::reader::queries::tests::get_todo`
Expected: FAIL.

- [ ] **Step 3: Add the query**

Update the `use` line at the top of `queries.rs` to include `ChecklistItem` and `TodoFull`:

```rust
use crate::core::types::{
    Area, ChecklistItem, Project, StartBucket, Tag, TaskStatus, TodoFull, TodoSummary,
};
```

Append to `queries.rs`:

```rust
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
```

- [ ] **Step 4: Run the new tests**

Run: `cargo test --lib core::reader::queries::tests::get_todo`
Expected: 2 passed.

- [ ] **Step 5: Create `tools/todos.rs`**

`crates/things-mcp/src/tools/todos.rs`:

```rust
//! Read tools that surface a single to-do.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::core::reader::queries::get_todo;
use crate::core::types::TodoFull;
use crate::state::AppState;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct GetTodoArgs {
    /// The to-do's UUID (`TMTask.uuid`).
    pub id: String,
}

pub async fn things_get_todo(
    state: AppState,
    args: GetTodoArgs,
) -> anyhow::Result<Option<TodoFull>> {
    let full = get_todo(&state.pool, args.id).await?;
    Ok(full)
}
```

- [ ] **Step 6: Register `tools::todos`**

`crates/things-mcp/src/tools/mod.rs`:

```rust
pub mod lists;
pub mod todos;
```

- [ ] **Step 7: Register the tool in `server.rs`**

Extend the imports and add the method:

```rust
use crate::core::types::{Area, Project, Tag, TodoFull, TodoSummary};
use crate::tools::todos::{things_get_todo, GetTodoArgs};

    #[tool(
        name = "things_get_todo",
        description = "Return a single to-do with notes, checklist, tags, and a repeating-template flag. Returns null if not found. Read-only.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn tool_get_todo(
        &self,
        Parameters(args): Parameters<GetTodoArgs>,
    ) -> Result<Json<Option<TodoFull>>, McpError> {
        let res = things_get_todo(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(res))
    }
```

- [ ] **Step 8: Build and run tests**

Run: `cargo build && cargo test`
Expected: clean.

- [ ] **Step 9: Commit**

```bash
git add crates/things-mcp/src
git commit -m "tools/todos: things_get_todo with checklist + repeating flag"
```

---

### Task 15: `things_get_project` (query + tool in `tools/projects.rs`)

`get_project` returns the project plus its child items grouped by heading. Introduce two new types — `Heading` and `ProjectFull` — in `core/types.rs`.

**Files:**
- Modify: `crates/things-mcp/src/core/types.rs`
- Modify: `crates/things-mcp/src/core/reader/queries.rs`
- Create: `crates/things-mcp/src/tools/projects.rs`
- Modify: `crates/things-mcp/src/tools/mod.rs`
- Modify: `crates/things-mcp/src/server.rs`

- [ ] **Step 1: Add the new types**

Append to `crates/things-mcp/src/core/types.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Heading {
    pub id: String,
    pub title: String,
    pub items: Vec<TodoSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProjectFull {
    #[serde(flatten)]
    pub project: Project,
    /// To-dos that live directly under the project (no heading).
    pub items: Vec<TodoSummary>,
    /// Headings, each carrying its own ordered child to-dos.
    pub headings: Vec<Heading>,
    pub completion_date: Option<String>,
    pub notes: Option<String>,
}
```

- [ ] **Step 2: Write the failing tests**

Append to the `tests` mod in `queries.rs`:

```rust
    #[tokio::test]
    async fn get_project_returns_full_shape_with_headings() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let full = get_project(&pool, "proj-1".to_string()).await.unwrap().unwrap();
        assert_eq!(full.project.title, "Reading list");
        assert_eq!(full.headings.len(), 1);
        assert_eq!(full.headings[0].title, "Articles");
        let head_items: Vec<_> = full.headings[0]
            .items
            .iter()
            .map(|i| i.title.as_str())
            .collect();
        assert_eq!(head_items, vec!["Read intro"]);
        // todo-4 lives directly under proj-1 (no heading)
        let direct_items: Vec<_> = full.items.iter().map(|i| i.title.as_str()).collect();
        assert!(direct_items.contains(&"Read RFC 9457"));
    }

    #[tokio::test]
    async fn get_project_returns_none_for_missing_id() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let res = get_project(&pool, "does-not-exist".to_string()).await.unwrap();
        assert!(res.is_none());
    }
```

- [ ] **Step 3: Run it to verify it fails**

Run: `cargo test --lib core::reader::queries::tests::get_project`
Expected: FAIL.

- [ ] **Step 4: Add the query**

Update the `use` line in `queries.rs` to include `Heading` and `ProjectFull`:

```rust
use crate::core::types::{
    Area, ChecklistItem, Heading, Project, ProjectFull, StartBucket, Tag, TaskStatus,
    TodoFull, TodoSummary,
};
```

Append to `queries.rs`:

```rust
pub async fn get_project(
    pool: &ReaderPool,
    id: String,
) -> Result<Option<ProjectFull>, ThingsError> {
    // 1. Project meta row.
    let id_for_meta = id.clone();
    let meta_sql = r#"
        SELECT t.uuid, t.title, t.area, t.status, t.notes, t.stopDate
        FROM TMTask AS t
        WHERE t.uuid = ?1 AND t.type = 1
    "#;
    let meta = pool
        .with_conn(move |c| -> rusqlite::Result<Option<(Project, Option<f64>)>> {
            let mut stmt = c.prepare_cached(meta_sql)?;
            let mut rows = stmt.query([id_for_meta.as_str()])?;
            if let Some(row) = rows.next()? {
                let project = Project {
                    id: row.get::<_, String>(0)?,
                    title: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    area_id: row.get::<_, Option<String>>(2)?,
                    status: TaskStatus::from_sqlite(row.get::<_, i64>(3)?),
                    notes: row.get::<_, Option<String>>(4)?,
                    tags: Vec::new(),
                };
                let stop_date: Option<f64> = row.get(5)?;
                Ok(Some((project, stop_date)))
            } else {
                Ok(None)
            }
        })
        .await?;
    let (mut project, stop_date) = match meta {
        Some(p) => p,
        None => return Ok(None),
    };

    // 2. Project tags via the same junction we use for to-dos.
    let tag_map = fetch_tags_for_tasks(pool, vec![id.clone()]).await?;
    if let Some(v) = tag_map.get(&id) {
        project.tags = v.clone();
    }

    // 3. All child rows (headings + to-dos) under the project, ordered by index.
    let id_for_children = id.clone();
    let children_sql = format!(
        r#"
        SELECT t.uuid, t.title, t.type, t.status, t.start, t.project, t.area, t.heading,
               t.startDate, t.deadline, t.creationDate, t.userModificationDate
        FROM TMTask AS t
        WHERE t.project = ?1 AND t.trashed = 0
        ORDER BY t."index"
        "#,
    );
    let children = pool
        .with_conn(move |c| -> rusqlite::Result<Vec<(i64, TodoSummary)>> {
            let mut stmt = c.prepare_cached(&children_sql)?;
            let iter = stmt.query_map([id_for_children.as_str()], |r| {
                let kind_int: i64 = r.get(2)?;
                // For headings we still call row_to_summary so we get the title/id; the kind
                // is returned alongside so the caller can split them.
                let summary = TodoSummary {
                    id: r.get::<_, String>(0)?,
                    title: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    status: TaskStatus::from_sqlite(r.get::<_, i64>(3)?),
                    start: StartBucket::from_sqlite(r.get::<_, i64>(4)?),
                    project_id: r.get::<_, Option<String>>(5)?,
                    area_id: r.get::<_, Option<String>>(6)?,
                    heading_id: r.get::<_, Option<String>>(7)?,
                    tags: Vec::new(),
                    scheduled: r
                        .get::<_, Option<i64>>(8)?
                        .and_then(crate::core::reader::dates::decode_things_date),
                    deadline: r
                        .get::<_, Option<i64>>(9)?
                        .and_then(crate::core::reader::dates::decode_things_date),
                    creation_date: r.get::<_, Option<f64>>(10)?.map(unix_to_iso),
                    modification_date: r.get::<_, Option<f64>>(11)?.map(unix_to_iso),
                };
                Ok((kind_int, summary))
            })?;
            iter.collect()
        })
        .await?;

    // 4. Split children into headings vs direct to-dos. For to-dos that point to
    //    a heading via `heading_id`, group them under that heading.
    let mut headings: std::collections::BTreeMap<String, Heading> = Default::default();
    let mut direct_items: Vec<TodoSummary> = Vec::new();
    let mut heading_order: Vec<String> = Vec::new();

    for (kind, summary) in children.iter() {
        if *kind == 2 {
            heading_order.push(summary.id.clone());
            headings.insert(
                summary.id.clone(),
                Heading {
                    id: summary.id.clone(),
                    title: summary.title.clone(),
                    items: Vec::new(),
                },
            );
        }
    }
    for (kind, summary) in children.into_iter() {
        if kind == 2 {
            continue;
        }
        match &summary.heading_id {
            Some(hid) if headings.contains_key(hid) => {
                headings.get_mut(hid).unwrap().items.push(summary);
            }
            _ => direct_items.push(summary),
        }
    }

    // 5. Attach tags onto the to-do summaries (direct + per-heading).
    let mut all_todo_ids: Vec<String> = direct_items.iter().map(|i| i.id.clone()).collect();
    for h in headings.values() {
        for i in &h.items {
            all_todo_ids.push(i.id.clone());
        }
    }
    let todo_tag_map = fetch_tags_for_tasks(pool, all_todo_ids).await?;
    for item in direct_items.iter_mut() {
        if let Some(v) = todo_tag_map.get(&item.id) {
            item.tags = v.clone();
        }
    }
    for h in headings.values_mut() {
        for item in h.items.iter_mut() {
            if let Some(v) = todo_tag_map.get(&item.id) {
                item.tags = v.clone();
            }
        }
    }

    let ordered_headings: Vec<Heading> =
        heading_order.into_iter().filter_map(|id| headings.remove(&id)).collect();

    Ok(Some(ProjectFull {
        project,
        items: direct_items,
        headings: ordered_headings,
        completion_date: stop_date.map(unix_to_iso),
        notes: None,
    }))
}
```

Note: `Project.notes` is already populated above; `ProjectFull.notes` mirrors it for callers that don't flatten — leave it `None` for now (Plan 3 search will revisit).

- [ ] **Step 5: Run the new tests**

Run: `cargo test --lib core::reader::queries::tests::get_project`
Expected: 2 passed.

- [ ] **Step 6: Create `tools/projects.rs`**

`crates/things-mcp/src/tools/projects.rs`:

```rust
//! Read tools that surface a single project.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::core::reader::queries::get_project;
use crate::core::types::ProjectFull;
use crate::state::AppState;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct GetProjectArgs {
    /// The project's UUID (`TMTask.uuid` where `type = 1`).
    pub id: String,
}

pub async fn things_get_project(
    state: AppState,
    args: GetProjectArgs,
) -> anyhow::Result<Option<ProjectFull>> {
    let full = get_project(&state.pool, args.id).await?;
    Ok(full)
}
```

- [ ] **Step 7: Register `tools::projects`**

`crates/things-mcp/src/tools/mod.rs`:

```rust
pub mod lists;
pub mod projects;
pub mod todos;
```

- [ ] **Step 8: Register the tool in `server.rs`**

Extend imports and add the method:

```rust
use crate::core::types::{Area, Project, ProjectFull, Tag, TodoFull, TodoSummary};
use crate::tools::projects::{things_get_project, GetProjectArgs};

    #[tool(
        name = "things_get_project",
        description = "Return a single project with its child to-dos and headings. Returns null if not found. Read-only.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn tool_get_project(
        &self,
        Parameters(args): Parameters<GetProjectArgs>,
    ) -> Result<Json<Option<ProjectFull>>, McpError> {
        let res = things_get_project(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(res))
    }
```

- [ ] **Step 9: Build and run tests**

Run: `cargo build && cargo test`
Expected: clean.

- [ ] **Step 10: Commit**

```bash
git add crates/things-mcp/src
git commit -m "tools/projects: things_get_project with headings grouping"
```

---

### Task 16: `things_list_by_tag` (query + tool with recursion)

**Files:**
- Modify: `crates/things-mcp/src/core/reader/queries.rs`
- Modify: `crates/things-mcp/src/tools/lists.rs`
- Modify: `crates/things-mcp/src/server.rs`

- [ ] **Step 1: Write the failing tests**

Append to the `tests` mod:

```rust
    #[tokio::test]
    async fn list_by_tag_non_recursive_returns_direct_matches_only() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        // 'Errand' is the parent tag. todo-2 is tagged 'Errand' directly;
        // todo-4 is tagged 'Call' (child of 'Errand') — without recurse, todo-4 is excluded.
        let rows = list_by_tag(
            &pool,
            ListByTagParams {
                tag: "Errand".to_string(),
                recurse: false,
                limit: 200,
            },
        )
        .await
        .unwrap();
        let titles: Vec<_> = rows.iter().map(|r| r.title.as_str()).collect();
        assert!(titles.contains(&"Call the dentist"));
        assert!(!titles.contains(&"Read RFC 9457"));
    }

    #[tokio::test]
    async fn list_by_tag_recursive_picks_up_child_tags() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let rows = list_by_tag(
            &pool,
            ListByTagParams {
                tag: "Errand".to_string(),
                recurse: true,
                limit: 200,
            },
        )
        .await
        .unwrap();
        let titles: Vec<_> = rows.iter().map(|r| r.title.as_str()).collect();
        assert!(titles.contains(&"Call the dentist"));
        assert!(titles.contains(&"Read RFC 9457"));
    }

    #[tokio::test]
    async fn list_by_tag_accepts_uuid_input_too() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let rows = list_by_tag(
            &pool,
            ListByTagParams {
                tag: "tag-deep".to_string(),
                recurse: false,
                limit: 200,
            },
        )
        .await
        .unwrap();
        let titles: Vec<_> = rows.iter().map(|r| r.title.as_str()).collect();
        assert_eq!(titles, vec!["Read research papers"]);
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test --lib core::reader::queries::tests::list_by_tag`
Expected: FAIL.

- [ ] **Step 3: Add the query**

Append to `queries.rs`:

```rust
pub struct ListByTagParams {
    pub tag: String,
    pub recurse: bool,
    pub limit: u32,
}

impl Default for ListByTagParams {
    fn default() -> Self {
        Self {
            tag: String::new(),
            recurse: true,
            limit: 200,
        }
    }
}

pub async fn list_by_tag(
    pool: &ReaderPool,
    params: ListByTagParams,
) -> Result<Vec<TodoSummary>, ThingsError> {
    let tag = params.tag.clone();
    let limit = params.limit as i64;
    let sql = if params.recurse {
        format!(
            r#"
            WITH RECURSIVE tag_tree(uuid) AS (
                SELECT uuid FROM TMTag WHERE title = ?1 OR uuid = ?1
                UNION ALL
                SELECT g.uuid FROM TMTag AS g JOIN tag_tree AS tt ON g.parent = tt.uuid
            )
            SELECT DISTINCT {SUMMARY_COLS}
            FROM TMTask AS t
            JOIN TMTaskTag AS tx ON tx.tasks = t.uuid
            JOIN tag_tree    ON tx.tags = tag_tree.uuid
            WHERE t.trashed = 0 AND t.type = 0
            ORDER BY t.creationDate DESC
            LIMIT ?2
            "#,
        )
    } else {
        format!(
            r#"
            SELECT DISTINCT {SUMMARY_COLS}
            FROM TMTask AS t
            JOIN TMTaskTag AS tx ON tx.tasks = t.uuid
            JOIN TMTag      AS g  ON g.uuid = tx.tags
            WHERE (g.title = ?1 OR g.uuid = ?1)
              AND t.trashed = 0
              AND t.type = 0
            ORDER BY t.creationDate DESC
            LIMIT ?2
            "#,
        )
    };

    let rows = pool
        .with_conn(move |c| -> rusqlite::Result<Vec<TodoSummary>> {
            let mut stmt = c.prepare_cached(&sql)?;
            let iter = stmt.query_map(
                rusqlite::params![tag, limit],
                row_to_summary,
            )?;
            iter.collect()
        })
        .await?;
    attach_tags(pool, rows).await
}
```

- [ ] **Step 4: Run the new tests**

Run: `cargo test --lib core::reader::queries::tests::list_by_tag`
Expected: 3 passed.

- [ ] **Step 5: Add the MCP tool layer**

Append to `tools/lists.rs`:

```rust
use crate::core::reader::queries::{list_by_tag, ListByTagParams};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct ListByTagArgs {
    /// Tag identifier — either the user-facing title (`"Errand"`) or the UUID (`"tag-errand"`).
    pub tag: String,
    /// If true (default), also matches descendants of the named tag.
    #[serde(default)]
    pub recurse: Option<bool>,
    /// Cap on returned rows. Defaults to 200.
    #[serde(default)]
    pub limit: Option<u32>,
}

pub async fn things_list_by_tag(
    state: AppState,
    args: ListByTagArgs,
) -> anyhow::Result<Vec<TodoSummary>> {
    let params = ListByTagParams {
        tag: args.tag,
        recurse: args.recurse.unwrap_or(true),
        limit: args.limit.unwrap_or(200),
    };
    let rows = list_by_tag(&state.pool, params).await?;
    Ok(rows)
}
```

- [ ] **Step 6: Register the tool**

In `server.rs`, extend the `use` line and add the method:

```rust
use crate::tools::lists::{
    things_list_anytime, things_list_areas, things_list_by_tag, things_list_inbox,
    things_list_logbook, things_list_projects, things_list_someday, things_list_tags,
    things_list_today, things_list_trash, things_list_upcoming, ListAnytimeArgs,
    ListAreasArgs, ListByTagArgs, ListInboxArgs, ListLogbookArgs, ListProjectsArgs,
    ListSomedayArgs, ListTagsArgs, ListTodayArgs, ListTrashArgs, ListUpcomingArgs,
};

    #[tool(
        name = "things_list_by_tag",
        description = "Return to-dos carrying a given tag. `tag` accepts the tag's title or UUID. With `recurse=true` (default), descendants of the tag are included. Read-only.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn tool_list_by_tag(
        &self,
        Parameters(args): Parameters<ListByTagArgs>,
    ) -> Result<Json<Vec<TodoSummary>>, McpError> {
        let rows = things_list_by_tag(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(rows))
    }
```

- [ ] **Step 7: Build and run tests**

Run: `cargo build && cargo test`
Expected: clean.

- [ ] **Step 8: Commit**

```bash
git add crates/things-mcp/src
git commit -m "tools/lists: things_list_by_tag with recursive option"
```

---

### Task 17: End-to-end integration test for the Plan-2 surface

**Files:**
- Create: `crates/things-mcp/tests/end_to_end_plan_2.rs`

- [ ] **Step 1: Write the integration test**

`crates/things-mcp/tests/end_to_end_plan_2.rs`:

```rust
//! End-to-end exercise of the Plan-2 read pipeline: build a fixture DB,
//! build AppState pointed at it, call each tool function the MCP server
//! delegates to, and assert the returned shape. Same approach as Plan 1's
//! `end_to_end_inbox.rs` — tests the library API one rung below MCP transport.

use things_mcp::core::reader::fixture::build_fixture;
use things_mcp::state::{AppState, AppStateOptions};
use things_mcp::tools::lists::{
    things_list_anytime, things_list_areas, things_list_by_tag, things_list_logbook,
    things_list_projects, things_list_someday, things_list_tags, things_list_today,
    things_list_trash, things_list_upcoming, ListAnytimeArgs, ListAreasArgs, ListByTagArgs,
    ListLogbookArgs, ListProjectsArgs, ListSomedayArgs, ListTagsArgs, ListTodayArgs,
    ListTrashArgs, ListUpcomingArgs,
};
use things_mcp::tools::projects::{things_get_project, GetProjectArgs};
use things_mcp::tools::todos::{things_get_todo, GetTodoArgs};

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
    // Keep tmp alive by leaking it into the state's pool path; tempdir is dropped
    // when the test function returns, but the DB file remains via the pool until
    // the state is dropped. Tests are short-lived so this is fine.
    std::mem::forget(tmp);
    state
}

#[tokio::test]
async fn plan_2_surface_returns_expected_shapes() {
    let state = build_state().await;

    // Lists
    let today = things_list_today(state.clone(), ListTodayArgs::default()).await.unwrap();
    assert!(today.iter().any(|t| t.title == "Today scheduled item"));

    let upcoming = things_list_upcoming(state.clone(), ListUpcomingArgs::default()).await.unwrap();
    assert!(upcoming.iter().any(|t| t.title == "Upcoming scheduled item"));
    assert!(upcoming.iter().any(|t| t.title == "Upcoming deadlined item"));

    let anytime = things_list_anytime(state.clone(), ListAnytimeArgs::default()).await.unwrap();
    assert!(anytime.iter().any(|t| t.title == "Read RFC 9457"));

    let someday = things_list_someday(state.clone(), ListSomedayArgs::default()).await.unwrap();
    assert!(someday.iter().any(|t| t.title == "Read research papers"));

    let logbook = things_list_logbook(state.clone(), ListLogbookArgs::default()).await.unwrap();
    assert!(logbook.iter().any(|t| t.title == "Old completed"));
    assert!(logbook.iter().any(|t| t.title == "Old canceled"));

    let trash = things_list_trash(state.clone(), ListTrashArgs::default()).await.unwrap();
    assert!(trash.iter().any(|t| t.title == "Trashed thing"));

    // Areas + projects + tags
    let areas = things_list_areas(state.clone(), ListAreasArgs::default()).await.unwrap();
    assert_eq!(areas.len(), 2);

    let projects_open = things_list_projects(state.clone(), ListProjectsArgs::default()).await.unwrap();
    assert!(projects_open.iter().any(|p| p.title == "Reading list"));

    let tags = things_list_tags(state.clone(), ListTagsArgs::default()).await.unwrap();
    assert_eq!(tags.len(), 3);
    assert!(tags.iter().any(|t| t.title == "Call" && t.parent_id.as_deref() == Some("tag-errand")));

    // Single-entity reads
    let todo = things_get_todo(
        state.clone(),
        GetTodoArgs {
            id: "todo-1".to_string(),
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(todo.summary.title, "Buy milk");
    assert_eq!(todo.checklist.len(), 3);

    let project = things_get_project(
        state.clone(),
        GetProjectArgs {
            id: "proj-1".to_string(),
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(project.headings.len(), 1);
    assert_eq!(project.headings[0].title, "Articles");

    // Tag drill-down
    let by_tag = things_list_by_tag(
        state.clone(),
        ListByTagArgs {
            tag: "Errand".to_string(),
            recurse: Some(true),
            limit: None,
        },
    )
    .await
    .unwrap();
    let titles: Vec<_> = by_tag.iter().map(|t| t.title.as_str()).collect();
    assert!(titles.contains(&"Call the dentist"));
    assert!(titles.contains(&"Read RFC 9457"));
}
```

- [ ] **Step 2: Run the integration test**

Run: `cargo test --test end_to_end_plan_2`
Expected: 1 passed.

- [ ] **Step 3: Run the whole suite**

Run: `cargo test`
Expected: all unit + integration tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/things-mcp/tests/end_to_end_plan_2.rs
git commit -m "tests: end-to-end exercise of plan-2 read surface"
```

---

### Task 18: Plan-2 wrap-up

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Mark Plan 2 status in the README**

Open `/Users/rjl/Code/mcp-things/README.md` and replace the status line:

```markdown
**Status:** Plan 2 — full read surface (`inbox`/`today`/`upcoming`/`anytime`/`someday`/`logbook`/`trash`/`areas`/`projects`/`tags`/`get_todo`/`get_project`/`list_by_tag`) over stdio. See `docs/superpowers/plans/` for the active plan and follow-ons.
```

- [ ] **Step 2: Run the full suite once more, including a release build**

Run: `cargo test && cargo build --release`
Expected: all tests pass; release build clean.

- [ ] **Step 3: Final commit**

```bash
git add README.md
git commit -m "docs: README — plan 2 read surface complete"
```

- [ ] **Step 4: Inspect the resulting history**

Run: `git log --oneline | head -20`
Expected: ~18 small commits, one per task.

---

## Self-review checklist (for the executor)

Once every task is complete, confirm against the spec (§4 read tools):

- [ ] Every read tool in spec §4 except `things_search` is registered in `ThingsServer`.
- [ ] Each tool carries the four MCP annotations (`read_only_hint = true`, `destructive_hint = false`, `idempotent_hint = true`, `open_world_hint = false`).
- [ ] `TodoSummary.scheduled` and `TodoSummary.deadline` populate from `decode_things_date` (no longer stubbed `None`).
- [ ] `things_list_tags` returns `parent_id` correctly so callers can rebuild the tree.
- [ ] `things_list_by_tag` with `recurse=true` matches descendants of the named tag (verified via the parent/child fixture rows).
- [ ] `things_get_project` groups child to-dos under headings by `heading_id`, preserves `"index"` order, and leaves project-direct items in `items[]`.
- [ ] Schema probe requires `TMChecklistItem.{uuid,title,status,task,index}` plus the new `"index"`/`todayIndex` columns on `TMTask`.
- [ ] Every commit message starts with a module prefix (`core/reader/...`, `tools/lists`, `tools/todos`, `tools/projects`, `tests`, `docs`).

When all green, **Plan 3** (search with FTS5 detection + LIKE fallback) is ready to start.
