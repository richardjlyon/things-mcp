# Plan 6 — tag CRUD via AppleScript + JSON URL

**Date:** 2026-05-20
**Predecessor:** Plan 5 (`2026-05-20-plan-5-write-tools-design.md`, shipped at `bd467fe`).
**Goal:** Eight MCP tools covering tag listing, assignment, creation, hierarchy mutation, rename/merge/delete — composed across the existing JSON URL writer chassis (assign/unassign) and a new AppleScript driver (admin ops).

---

## Architecture

### Two execution backends

1. **JSON URL chassis (existing, from Plan 5).** `Writer::fire(op, Some(predicate))` handles `assign`/`unassign` because the Things URL scheme has `add-tags` and supports tag replacement via `update` ops. Verified by polling SQLite.
2. **AppleScript driver (new).** A small `core/applescript/` module — driver trait + production `osascript` impl + recording test impl + script-render helpers + `TagAdmin` facade. Handles `create`/`rename`/`merge`/`delete`/`move` because Things' JSON URL has no global tag-admin operations. Verification is the osascript exit code — synchronous, no SQLite poll needed.

`assign` is a single JSON URL `update { tags: add-tags }` op. `unassign` is a read-modify-write: read current tags from SQLite, compute `new = current - [tag]`, fire JSON URL `update { tags: new }`. The unassign race window (~100–300 ms) is **documented in the tool description**; this is the explicit Plan-6 decision over AppleScript-atomic removal.

### Module layout

```
crates/things-mcp/src/
├── core/
│   ├── applescript/                 ← NEW
│   │   ├── mod.rs                   re-exports
│   │   ├── driver.rs                AppleScriptDriver trait + 2 impls
│   │   ├── script.rs                pure render_* helpers (no I/O)
│   │   └── admin.rs                 TagAdmin facade + TagOutcome
│   ├── reader/
│   │   └── tags.rs                  ← NEW list_tags + build_tree
│   └── writer/
│       └── verify.rs                + TagOnTodoById variant
├── tools/
│   ├── tags.rs                      ← NEW 6 admin + list adapters
│   └── todos.rs                     + things_assign_tag, things_unassign_tag
└── state.rs                         + AppState.tag_admin
```

---

## Tool surface (8 tools)

| Tool | Backend | Args | Verify |
|---|---|---|---|
| `things_list_tags` | SQLite read | _none_ | n/a (read-only) |
| `things_assign_tag` | JSON URL (`add-tags`) | `id: String`, `tags: Vec<String>` | `TagOnTodoById { id, tag: tags[0], present: true }` |
| `things_unassign_tag` | JSON URL (read-modify-write) | `id: String`, `tags: Vec<String>` | `TagOnTodoById { id, tag: tags[0], present: false }` |
| `things_create_tag` | AppleScript | `name: String`, `parent: Option<String>` | osascript exit code |
| `things_rename_tag` | AppleScript | `old: String`, `new: String` | osascript exit code |
| `things_merge_tags` | AppleScript | `source: String`, `target: String` | osascript exit code |
| `things_delete_tag` | AppleScript | `name: String` | osascript exit code |
| `things_move_tag` | AppleScript | `name: String`, `new_parent: Option<String>` (None → promote to root) | osascript exit code |

**Tag identifier:** by name (not uuid). Things' JSON URL requires names; AppleScript's `tag named "X"` supports it natively; UUID-keying would force an extra lookup per call for no gain.

**Assign/unassign target id:** single polymorphic `id` — works for both to-dos and projects. Tool description warns against passing heading ids.

**Per-tag predicate, per fire.** For multi-tag assign/unassign, only the first tag is verified. Things merges all tags in one JSON URL op; once one lands, we trust the rest landed in the same write. Bounds verify latency.

**Output shapes:**
- 2 JSON URL write tools (`assign`/`unassign`) return `WriteOutcome` (existing Plan 5 shape).
- 5 AppleScript write tools (`create`/`rename`/`merge`/`delete`/`move`) return `TagOutcome` (new — defined in the `admin.rs` section).
- `things_list_tags` returns `Json<TagListing>` (defined in the "Tag listing reader" section).

**MCP annotations:**

| Tool | read_only | destructive | idempotent | open_world |
|---|---|---|---|---|
| `things_list_tags` | true | false | true | false |
| `things_assign_tag` | false | false | false | true |
| `things_unassign_tag` | false | false | false | true |
| `things_create_tag` | false | false | false | true |
| `things_rename_tag` | false | true | false | true |
| `things_merge_tags` | false | true | false | true |
| `things_delete_tag` | false | true | false | true |
| `things_move_tag` | false | false | false | true |

---

## `core/applescript/` module

### `driver.rs`

```rust
#[async_trait]
pub trait AppleScriptDriver: Send + Sync + std::fmt::Debug {
    /// Run the given AppleScript source. Returns stdout on success;
    /// `ThingsError::AppleScriptFailed { stderr, exit }` on non-zero exit.
    async fn run(&self, script: &str) -> Result<String, ThingsError>;
}

#[derive(Debug, Default)]
pub struct OsascriptDriver;

#[derive(Debug, Default)]
pub struct RecordingAppleScript {
    pub scripts: Mutex<Vec<String>>,
    pub responses: Mutex<VecDeque<Result<String, ThingsError>>>,
}
```

- `OsascriptDriver::run` spawns `osascript -e <script>` via `tokio::process::Command`. Captures stdout/stderr; returns `ThingsError::AppleScriptFailed { stderr, exit }` on non-zero exit.
- `RecordingAppleScript::run` appends the script to `scripts`, pops the next queued `responses` (default `Ok(String::new())` if queue empty). Tests assert on the recorded script string and seed expected outcomes via `push_response()`.

**Things-not-running:** if Things isn't launched, `osascript`'s `tell application "Things3"` block transparently launches it — same as the URL scheme's `open -g`. No explicit "is Things running" probe needed; the existing startup `schema_probe` already covers DB-side health.

### `script.rs`

Pure render functions (no I/O). One per admin op:

- `render_create_tag(name, parent: Option<&str>) -> String`
- `render_rename_tag(old, new) -> String`
- `render_merge_tags(source, target) -> String`
- `render_delete_tag(name) -> String`
- `render_move_tag(name, new_parent: Option<&str>) -> String`

Each wraps its body in a `tell application "Things3" \n … \n end tell` block. Names get escaped via a small helper: `escape_applescript_string(s) -> String` (double quotes → `\"`, backslashes → `\\`). One unit test per render fn for nominal + quote-in-name + None parent.

### `admin.rs` — `TagAdmin` facade

```rust
pub struct TagAdmin {
    pub driver: Arc<dyn AppleScriptDriver>,
    pub safety: SafetyMode,
}

impl TagAdmin {
    pub async fn create(&self, name: &str, parent: Option<&str>) -> Result<TagOutcome, ThingsError>;
    pub async fn rename(&self, old: &str, new: &str) -> Result<TagOutcome, ThingsError>;
    pub async fn merge(&self, source: &str, target: &str) -> Result<TagOutcome, ThingsError>;
    pub async fn delete(&self, name: &str) -> Result<TagOutcome, ThingsError>;
    pub async fn move_under(&self, name: &str, new_parent: Option<&str>) -> Result<TagOutcome, ThingsError>;
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct TagOutcome {
    pub action: String,
    pub dry_run: bool,
    pub latency_ms: u64,
    /// First line of osascript stdout, truncated to 200 chars. Empty on no-op.
    pub osascript_stdout: String,
}
```

**Safety gate** at the top of each method:

```rust
if matches!(self.safety, SafetyMode::DryRun) {
    return Ok(TagOutcome { dry_run: true, action, latency_ms: 0, osascript_stdout: String::new() });
}
if matches!(self.safety, SafetyMode::Forbidden) {
    return Err(ThingsError::TestDbWriteForbidden);
}
```

Same shape as Plan 5's `Writer::fire`. Auth-token gate **not** checked — AppleScript doesn't use it.

**Defense-in-depth validation** inside `TagAdmin` itself: `merge(source, target)` rejects `source == target` (also rejected at the tool-adapter layer). Other input validation lives in the tool adapters.

**Why `TagOutcome` instead of reusing `WriteOutcome`?** `WriteOutcome.id` and `WriteOutcome.verified` don't fit AppleScript ops (tags aren't addressed by uuid in the JSON URL API; "verified" is conflated with SQLite polling). Cleaner to add a focused type than to twist the existing one.

---

## Verify additions

One new `VerifyPredicate` variant in `core/writer/verify.rs`:

```rust
    /// Confirms that the row at `id` either has (or doesn't have) the
    /// given tag, depending on `present`. Used by assign/unassign.
    TagOnTodoById {
        id: String,
        tag: String,
        present: bool,
    },
```

**Existence-probe extension** at the top of `verify()`: extend the OR-pattern to include `TagOnTodoById { id, .. }`. Same short-circuit-to-NotFound on missing uuid.

**`check_once` arm** joins `TMTaskTag` (the Things join table) to `TMTag`:

```rust
VerifyPredicate::TagOnTodoById { id, tag, present } => {
    let sql = r#"
        SELECT EXISTS (
            SELECT 1
            FROM TMTaskTag tt
            JOIN TMTag t ON t.uuid = tt.tags
            WHERE tt.tasks = ? AND t.title = ?
        )
    "#;
    let mut stmt = c.prepare_cached(sql)?;
    let has_tag: bool = stmt
        .query_row(rusqlite::params![id, tag], |r| r.get::<_, i64>(0).map(|n| n != 0))?;

    if has_tag == *present {
        let summary = read_summary_by_id(c, id)?;
        return Ok(Some(summary));
    }
    Ok(None)
}
```

Schema assumption: join table is `TMTaskTag(tasks, tags)` — both uuid foreign keys. The `schema_probe` at startup confirms this; if absent, startup fails with a clear error.

`read_summary_by_id(c, id)` is the existing helper that runs `SELECT {SUMMARY_COLS} FROM TMTask WHERE uuid=? AND trashed=0`. If it doesn't already exist in factored form, this plan extracts it.

**Two new verify tests:**
- `verify_tag_on_todo_by_id_matches_when_present_true_and_tag_set` — fixture row with tag, assert Verified with `present: true`.
- `verify_tag_on_todo_by_id_matches_when_present_false_and_tag_absent` — fixture row without tag, assert Verified with `present: false`.

---

## Tag listing reader

`core/reader/tags.rs` (new).

```rust
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Tag {
    pub uuid: String,
    pub name: String,
    pub parent_uuid: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct TagNode {
    pub uuid: String,
    pub name: String,
    pub children: Vec<TagNode>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct TagListing {
    pub flat: Vec<Tag>,
    pub roots: Vec<TagNode>,
}

pub async fn list_tags(pool: &SqlitePool) -> Result<TagListing, ThingsError>;
```

**Query:** single `SELECT uuid, title, parent FROM TMTag ORDER BY title COLLATE NOCASE`. Tags table is small (typically <100 rows); no pagination, no filtering.

**Tree construction** in Rust: `build_tree(flat: &[Tag]) -> Vec<TagNode>`. Group by `parent_uuid`, recurse from roots.

**Cycle safety:** the recursion carries a `visited: HashSet<&str>` guard. On a cycle, return early without including the offending node and emit `tracing::warn!("tag cycle detected at uuid={uuid}")`. Trades one HashSet allocation for crash-immunity against a corrupt DB.

**Three unit tests:**
- `list_tags_returns_flat_and_roots_for_fixture` — full integration with the fixture.
- `build_tree_handles_two_level_nesting` — pure unit on `build_tree` with a synthetic `Vec<Tag>`.
- `build_tree_handles_cycle_without_looping` — synthetic cycle; assert no infinite loop + warning emitted.

---

## Fixture extension

`core/reader/fixture.rs::build_fixture` gains:
- 2 root tags: "Work" (`tag-1`), "Errands" (`tag-2`).
- 1 child tag: "Urgent" (`tag-3`, parent `tag-1`).
- 1 `TMTaskTag` row: `(todo-2, tag-2)` — gives the verify-test fixture a row with a tag.

todo-1 ("Buy milk") stays untagged for the `present: false` verify test.

---

## State wiring

```rust
// state.rs
pub struct AppState {
    pub pool: Arc<SqlitePool>,
    pub writer: Arc<Writer>,
    pub tag_admin: Arc<TagAdmin>,  // ← NEW
    // …
}

pub struct AppStateOptions {
    // …existing fields…
    pub executor_override: Option<Arc<dyn Executor>>,
    pub applescript_override: Option<Arc<dyn AppleScriptDriver>>,  // ← NEW
}
```

`AppState::build` constructs `TagAdmin` from `cfg.safety` + (overridable) driver. The 5 AppleScript-backed tool adapters call `state.tag_admin.<method>`; `assign`/`unassign` continue to call `state.writer.fire(op, Some(predicate))`.

---

## Error handling

**No new `ThingsError` variants.** Plan 6 reuses:
- `AppleScriptFailed { stderr, exit }` (existing, line 55 of `core/error.rs`) — surfaces osascript failures verbatim.
- `InvalidInput { field, reason }` (existing, Plan 2–3) — used by tool adapters for empty names, self-merge, no-op rename, empty tag list.
- `TestDbWriteForbidden` (existing, Plan 4) — covers `Forbidden` safety mode.

**Input validation surface (tool-adapter layer):**

| Tool | Rejection |
|---|---|
| All admin ops | empty `name` |
| `assign`/`unassign` | empty `id`, empty `tags` vec |
| `merge` | `source == target` |
| `rename` | `old == new` (no-op) |
| `create` | empty `name`; empty `parent` if `Some("")` |
| `move` | empty `name` |

---

## Test strategy

| Layer | Test count |
|---|---|
| Script rendering (pure, `script.rs`) | ~12 (one per render fn × nominal + quote-edge case + None-parent) |
| Driver (`driver.rs`) | ~3 (RecordingAppleScript record/replay + 1 ignored OsascriptDriver smoke) |
| `TagAdmin` (`admin.rs`) | ~10 (dry-run short-circuit + live calls correct script + Forbidden errors) |
| `core/reader/tags.rs` | ~3 (list + tree shape + cycle) |
| `verify.rs` `TagOnTodoById` | 2 |
| Integration (`tests/end_to_end_tags_plan_6.rs`) | ~9 (8 dry-run + 1 live-mode with RecordingAppleScript) |
| **Total delta** | **~39 tests** (112 → ~151) |

The live-mode integration test exercises the full path for one tool (`things_rename_tag`): builds AppState with `applescript_override: Some(RecordingAppleScript)`, calls the tool, asserts the recorded script string matches `render_rename_tag(old, new)`.

---

## File map

**Create (7 files):**
- `crates/things-mcp/src/core/applescript/mod.rs`
- `crates/things-mcp/src/core/applescript/driver.rs`
- `crates/things-mcp/src/core/applescript/script.rs`
- `crates/things-mcp/src/core/applescript/admin.rs`
- `crates/things-mcp/src/core/reader/tags.rs`
- `crates/things-mcp/src/tools/tags.rs`
- `crates/things-mcp/tests/end_to_end_tags_plan_6.rs`

(If `admin.rs` grows past ~250 lines, factor `TagOutcome` into `core/applescript/outcome.rs`.)

**Modify (8 files):**
- `crates/things-mcp/src/core/writer/verify.rs` — `TagOnTodoById` predicate + arm + 2 tests
- `crates/things-mcp/src/core/reader/mod.rs` — `pub mod tags;`
- `crates/things-mcp/src/core/reader/fixture.rs` — 3 tag rows + 1 TMTaskTag row
- `crates/things-mcp/src/core/mod.rs` — `pub mod applescript;`
- `crates/things-mcp/src/state.rs` — `tag_admin` field + `applescript_override` option
- `crates/things-mcp/src/tools/mod.rs` — `pub mod tags;`
- `crates/things-mcp/src/tools/todos.rs` — `things_assign_tag` + `things_unassign_tag`
- `crates/things-mcp/src/server.rs` — 8 `#[tool]` registrations
- `README.md` — status line bump

**No new dependencies. No new `ThingsError` variants.**

---

## Out of scope (deferred)

- Tag-aware FTS — already covered by `things_search` (Plan 3).
- Bulk tag operations — `things_bulk_json` (Plan 5) covers this if needed.
- AppleScript timeout / interrupt handling — `osascript` blocks until done; if Things hangs, the tool hangs. If it becomes a problem, wrap with `tokio::time::timeout`.
- UUID-keyed admin ops (e.g. `rename_tag_by_uuid`). Names are unambiguous in Things; adding a parallel uuid-keyed surface doubles the tool count for no gain.

---

## Open questions (resolve during planning, not now)

1. **Does `core/reader/` already factor out `read_summary_by_id`?** If not, the plan extracts it. (Inspection during plan-writing.)
2. **Does `tools/todos.rs` already export the `UpdateTodoArgs` shape for the tags-only diff?** Probably yes (Plan 5 wired it). Confirms whether `things_assign_tag` reuses it or needs a narrower `TagsArgs`.
3. **Should `things_list_tags` cache results?** No for v1 — invalidation cost outweighs the savings on a <100-row table. Re-evaluate if telemetry shows the query as a hotspot.
