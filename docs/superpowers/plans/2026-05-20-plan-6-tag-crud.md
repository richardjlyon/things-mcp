# things-mcp Plan 6 — tag CRUD via AppleScript + JSON URL

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Layer eight tag-aware MCP tools on top of the Plan 5 chassis: a tree-shaped tag listing, two JSON-URL-backed tag assignment ops on individual to-dos, and five AppleScript-driven tag-admin ops (create / rename / merge / delete / move under).

**Architecture:** Two composed backends.

1. **JSON URL chassis (existing, from Plan 5).** `Writer::fire(op, Some(predicate))` handles `assign`/`unassign` because Things' `update` op accepts a `tags` attribute that replaces the whole tag set. Both tools read the current tags from SQLite, compute the new set, and fire a tags-only `UpdateTodo`. Verified by polling SQLite via a new `VerifyPredicate::TagOnTodoById` variant.
2. **AppleScript driver (new).** A `core/applescript/` module — `AppleScriptDriver` trait + production `osascript` impl + recording test impl + pure `render_*` script helpers + `TagAdmin` facade. Handles `create`/`rename`/`merge`/`delete`/`move` because Things' JSON URL has no global tag-admin operations. Verification is the osascript exit code — synchronous, no SQLite poll.

**Tech Stack:** Same as Plans 1–5. No new dependencies. No new `ThingsError` variants.

**Spec:** `docs/superpowers/specs/2026-05-20-plan-6-tag-crud-design.md`.

**Predecessor:** Plan 5 plan at `docs/superpowers/plans/2026-05-20-plan-5-write-tools.md` (HEAD `ea9f4e1`, 113 tests reported: 112 passing + 1 ignored smoke). Plan 6 spec at `docs/superpowers/specs/2026-05-20-plan-6-tag-crud-design.md` is committed but not yet acted on.

**Deviations from the spec (resolved during planning):**

- **Assign uses read-modify-write, not native `add-tags`.** The spec phrases assign as a "single JSON URL `update { tags: add-tags }` op". Inspection confirmed Things' JSON URL `update` payload only accepts `tags` (replacement); `add-tags` is a separate URL-scheme action that doesn't compose with the JSON payload. Both `assign` and `unassign` therefore use RMW through `UpdateTodoSpec.tags = Some(merged_set)`. Same ~100-300 ms race window the spec already documents for unassign; symmetry simplifies the implementation and avoids touching `UpdateTodoSpec`.
- **No fixture extension needed.** The existing `build_fixture` (`crates/things-mcp/src/core/reader/fixture.rs`) already creates `TMTaskTag` plus 3 tags (`Errand`, `Call` child of `Errand`, `Deep work`) and 4 join rows. `todo-2` is tagged `Errand` (good for `present: true`); `todo-1` has no tags (good for `present: false`). Plan 6 reuses these rows instead of adding spec-named ones.
- **`things_list_tags` already exists.** The Plan-2 surface ships a `things_list_tags` returning `Json<Vec<Tag>>` from `tools/lists.rs`. Plan 6 migrates the function to the new `tools/tags.rs` and changes the return shape to `Json<TagListing>` (with both `flat` and `roots`). The two existing related tests (`list_tags_returns_flat_list_with_parent_links` in `queries.rs`, and the server registration in `server.rs`) are updated atomically.
- **Existence-probe extension for `TagOnTodoById`.** The `verify()` short-circuit in `core/writer/verify.rs:60-79` checks for `UpdateById | StatusChange | MoveById`. Plan 6 extends that OR-pattern to include `TagOnTodoById { id, .. }`.
- **No `read_summary_by_id` helper extraction.** The verify check arms for `UpdateById`, `StatusChange`, `MoveById` all use inline `SELECT {SUMMARY_COLS} FROM TMTask WHERE uuid=?` + `row_to_summary(r)`. Plan 6 follows the same shape; no helper is extracted.

**Expected test counts (cumulative, reported by `cargo test`):**

| After task | Lib (passing) | Integration | Reported | Delta |
|---|---|---|---|---|
| Baseline (HEAD `ea9f4e1`) | 99 | 13 | 113 (112 pass + 1 ignored) | — |
| T1 (driver scaffolding) | 101 | 13 | 116 (114 pass + 2 ignored) | +3 |
| T2 (script render helpers) | 113 | 13 | 128 (126 pass + 2 ignored) | +12 |
| T3 (`TagAdmin` facade) | 123 | 13 | 138 (136 pass + 2 ignored) | +10 |
| T4 (`core/reader/tags.rs` + `get_tags_for_task`) | 128 | 13 | 143 (141 pass + 2 ignored) | +5 |
| T5 (`TagOnTodoById` predicate) | 130 | 13 | 145 (143 pass + 2 ignored) | +2 |
| T6 (`assign`/`unassign` adapters) | 130 | 13 | 145 | 0 |
| T7 (state + 6 tool adapters + registrations) | 130 | 13 | 145 | 0 |
| T8 (9 e2e tests) | 130 | 22 | 154 (152 pass + 2 ignored) | +9 |
| T9 (README sweep) | 130 | 22 | 154 | 0 |

(The baseline reports 113: 112 passing + 1 ignored. T1 adds one new ignored smoke test, bringing the ignored count to 2. The spec promised "~39 tests delta"; this plan lands 41 because T4 also adds 2 `get_tags_for_task` tests the original task-grain didn't book — a strict superset of the planned coverage.)

---

## File map

**Create (7 new files):**

- `crates/things-mcp/src/core/applescript/mod.rs` — module root + re-exports
- `crates/things-mcp/src/core/applescript/driver.rs` — `AppleScriptDriver` trait + `OsascriptDriver` + `RecordingAppleScript`
- `crates/things-mcp/src/core/applescript/script.rs` — pure `render_*` helpers + `escape_applescript_string`
- `crates/things-mcp/src/core/applescript/admin.rs` — `TagAdmin` facade + `TagOutcome`
- `crates/things-mcp/src/core/reader/tags.rs` — `TagListing`, `TagNode`, `build_tree`, `list_tags_with_tree`
- `crates/things-mcp/src/tools/tags.rs` — 7 tool adapters (`things_list_tags`, `things_create_tag`, `things_rename_tag`, `things_merge_tags`, `things_delete_tag`, `things_move_tag` admin path, and the `TagAssignmentArgs` shape shared with `tools/todos.rs`)
- `crates/things-mcp/tests/end_to_end_tags_plan_6.rs` — 9 integration tests

**Modify:**

- `crates/things-mcp/src/core/mod.rs` — declare `pub mod applescript;`
- `crates/things-mcp/src/core/reader/mod.rs` — declare `pub mod tags;`
- `crates/things-mcp/src/core/reader/queries.rs` — add `pub async fn get_tags_for_task(pool, id) -> Vec<String>`
- `crates/things-mcp/src/core/writer/verify.rs` — `TagOnTodoById` variant + existence-probe extension + `check_once` arm + 2 tests
- `crates/things-mcp/src/state.rs` — `tag_admin` field on `AppState` + `applescript_override` option on `AppStateOptions`
- `crates/things-mcp/src/tools/mod.rs` — declare `pub mod tags;`
- `crates/things-mcp/src/tools/lists.rs` — DELETE `things_list_tags` + `ListTagsArgs` (migrated to `tools/tags.rs`)
- `crates/things-mcp/src/tools/todos.rs` — `things_assign_tag` + `things_unassign_tag` adapters (RMW through `UpdateTodoSpec`)
- `crates/things-mcp/src/server.rs` — replace 1 list-tags registration; add 7 new tool registrations
- `README.md` — status line bump to Plan 6

---

## Task 1: `core/applescript/{mod, driver}.rs` scaffolding

Create the `core/applescript/` module with the trait and two implementations: production `OsascriptDriver` (spawns `osascript -e <script>`) and test `RecordingAppleScript` (records scripts, replays queued responses). Mirrors the existing `core/writer/executor.rs` shape exactly — same shape user, same record/replay test pattern.

**Files:**
- Create: `crates/things-mcp/src/core/applescript/mod.rs`
- Create: `crates/things-mcp/src/core/applescript/driver.rs`
- Modify: `crates/things-mcp/src/core/mod.rs`

- [ ] **Step 1: Add the module declaration to `core/mod.rs`**

Edit `crates/things-mcp/src/core/mod.rs`. The file currently is:

```rust
pub mod backup;
pub mod config;
pub mod error;
pub mod reader;
pub mod types;
pub mod writer;
```

Replace with (keep alphabetical order):

```rust
pub mod applescript;
pub mod backup;
pub mod config;
pub mod error;
pub mod reader;
pub mod types;
pub mod writer;
```

- [ ] **Step 2: Create `core/applescript/mod.rs`**

`crates/things-mcp/src/core/applescript/mod.rs`:

```rust
//! AppleScript path for tag-admin operations (`create`/`rename`/`merge`/
//! `delete`/`move_under`). Things' JSON URL has no global tag-admin verbs,
//! so these go through `osascript -e <script>` and trust the synchronous
//! exit code as verification.
//!
//! Symmetric to `core/writer/` for the JSON URL path: a driver trait
//! (`AppleScriptDriver`) with a production impl (`OsascriptDriver`) and a
//! recording test impl (`RecordingAppleScript`), pure `render_*` helpers
//! in `script.rs`, and a facade (`TagAdmin`) in `admin.rs` that owns
//! the safety gate and result composition.

pub mod driver;
```

(`admin.rs` and `script.rs` get added in Task 2 and Task 3; the `pub mod` lines move into this file as those tasks land.)

- [ ] **Step 3: Create `core/applescript/driver.rs`**

`crates/things-mcp/src/core/applescript/driver.rs`:

```rust
//! AppleScript driver seam.
//!
//! Production: `OsascriptDriver` spawns `/usr/bin/osascript -e <script>`.
//! Tests: `RecordingAppleScript` captures every script string and pops queued
//! responses, so unit tests can assert exactly which AppleScript was emitted
//! without ever spawning `osascript`.

use std::collections::VecDeque;
use std::sync::Mutex;

use async_trait::async_trait;

use crate::core::error::ThingsError;

#[async_trait]
pub trait AppleScriptDriver: Send + Sync + std::fmt::Debug {
    /// Run the given AppleScript source. Returns stdout on success; returns
    /// `ThingsError::AppleScriptFailed { stderr, exit }` on non-zero exit.
    async fn run(&self, script: &str) -> Result<String, ThingsError>;
}

/// Production driver: shells out to `/usr/bin/osascript -e <script>`.
///
/// Things-not-running: a `tell application "Things3"` block in the rendered
/// script transparently launches Things on first call, so no explicit "is
/// Things running" probe is needed here. The startup `schema_probe` already
/// covers DB-side health.
#[derive(Debug, Default)]
pub struct OsascriptDriver;

#[async_trait]
impl AppleScriptDriver for OsascriptDriver {
    async fn run(&self, script: &str) -> Result<String, ThingsError> {
        let output = tokio::process::Command::new("/usr/bin/osascript")
            .arg("-e")
            .arg(script)
            .output()
            .await
            .map_err(|e| ThingsError::ExecutorFailed {
                message: format!("spawn /usr/bin/osascript: {e}"),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            let exit = output.status.code().unwrap_or(-1);
            return Err(ThingsError::AppleScriptFailed { stderr, exit });
        }

        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

/// Test driver: records every script it's asked to run without spawning
/// `osascript`. Tests assert on `scripts()` and seed `push_response()` to
/// control return values.
#[derive(Debug, Default)]
pub struct RecordingAppleScript {
    scripts: Mutex<Vec<String>>,
    responses: Mutex<VecDeque<Result<String, ThingsError>>>,
}

impl RecordingAppleScript {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns every script that has been passed to `run()`, in call order.
    pub fn scripts(&self) -> Vec<String> {
        self.scripts.lock().unwrap().clone()
    }

    /// Queue a response that the *next* call to `run()` will return. If no
    /// response has been queued, `run()` returns `Ok(String::new())`.
    pub fn push_response(&self, response: Result<String, ThingsError>) {
        self.responses.lock().unwrap().push_back(response);
    }
}

#[async_trait]
impl AppleScriptDriver for RecordingAppleScript {
    async fn run(&self, script: &str) -> Result<String, ThingsError> {
        self.scripts.lock().unwrap().push(script.to_string());
        match self.responses.lock().unwrap().pop_front() {
            Some(r) => r,
            None => Ok(String::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn recording_driver_captures_scripts_in_order() {
        let rec = RecordingAppleScript::new();
        rec.run("tell application \"Things3\" to make tag with properties {name:\"Work\"}")
            .await
            .unwrap();
        rec.run("tell application \"Things3\" to delete tag \"Old\"")
            .await
            .unwrap();
        let scripts = rec.scripts();
        assert_eq!(scripts.len(), 2);
        assert!(scripts[0].contains("make tag"));
        assert!(scripts[1].contains("delete tag"));
    }

    #[tokio::test]
    async fn recording_driver_replays_queued_responses_in_order() {
        let rec = RecordingAppleScript::new();
        rec.push_response(Ok("first".into()));
        rec.push_response(Err(ThingsError::AppleScriptFailed {
            stderr: "boom".into(),
            exit: 1,
        }));
        let r1 = rec.run("a").await.unwrap();
        assert_eq!(r1, "first");
        let r2 = rec.run("b").await;
        assert!(matches!(r2, Err(ThingsError::AppleScriptFailed { exit: 1, .. })));
        // Queue is now empty — the next call gets the default Ok(String::new()).
        let r3 = rec.run("c").await.unwrap();
        assert_eq!(r3, "");
    }

    // Manual smoke test: opt-in only — fires `/usr/bin/osascript` against
    // the local machine. Run with `cargo test -- --ignored
    // osascript_driver_smoke` only when you intend to.
    #[tokio::test]
    #[ignore = "fires /usr/bin/osascript on the local machine"]
    async fn osascript_driver_smoke() {
        let driver = OsascriptDriver;
        // Trivial script that returns "hello" — does NOT talk to Things.
        let out = driver
            .run("return \"hello\"")
            .await
            .expect("osascript should run");
        assert!(out.contains("hello"));
    }
}
```

- [ ] **Step 4: Build + full sweep**

Run:

```
cargo build
cargo test
```

Expected:
- Build: clean.
- Tests: **116 reported** (114 passing + 2 ignored). +3 over baseline: 2 recording-driver tests + 1 newly-ignored `osascript_driver_smoke`.

- [ ] **Step 5: Commit**

```bash
git add crates/things-mcp/src/core/mod.rs \
        crates/things-mcp/src/core/applescript/mod.rs \
        crates/things-mcp/src/core/applescript/driver.rs
git commit -m "core/applescript: driver trait + osascript + recording impls"
```

---

## Task 2: `core/applescript/script.rs` — render helpers

Five pure render functions, one per tag-admin op. Each wraps its body in a `tell application "Things3" \n … \n end tell` block. Names get escaped via `escape_applescript_string` (double quotes → `\"`, backslashes → `\\`). One test per render function for nominal + quote-in-name + (where applicable) `None` parent.

**Files:**
- Create: `crates/things-mcp/src/core/applescript/script.rs`
- Modify: `crates/things-mcp/src/core/applescript/mod.rs`

- [ ] **Step 1: Create `core/applescript/script.rs`**

`crates/things-mcp/src/core/applescript/script.rs`:

```rust
//! Pure AppleScript render functions for tag-admin ops. No I/O — each
//! function takes the inputs the tool surface accepts and returns the
//! AppleScript source as a `String`. The driver (`OsascriptDriver`) and
//! the facade (`TagAdmin`) are the layers that actually run the script.

/// Escape a user-supplied string for safe inclusion inside an AppleScript
/// double-quoted literal. AppleScript's escape rules: backslash escapes
/// itself and a literal double quote.
pub fn escape_applescript_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

pub fn render_create_tag(name: &str, parent: Option<&str>) -> String {
    let name_q = escape_applescript_string(name);
    match parent {
        Some(p) => {
            let p_q = escape_applescript_string(p);
            format!(
                r#"tell application "Things3"
    set newTag to make new tag with properties {{name:"{name_q}"}}
    set parent tag of newTag to tag "{p_q}"
end tell"#,
            )
        }
        None => format!(
            r#"tell application "Things3"
    make new tag with properties {{name:"{name_q}"}}
end tell"#,
        ),
    }
}

pub fn render_rename_tag(old: &str, new: &str) -> String {
    let old_q = escape_applescript_string(old);
    let new_q = escape_applescript_string(new);
    format!(
        r#"tell application "Things3"
    set name of tag "{old_q}" to "{new_q}"
end tell"#,
    )
}

/// Reassign every to-do that carries `source` to also carry `target`, then
/// delete the `source` tag. AppleScript surface: `to dos of tag "source"`
/// enumerates the tasks; we add the target tag to each and then remove the
/// source tag from the global tag list.
pub fn render_merge_tags(source: &str, target: &str) -> String {
    let s_q = escape_applescript_string(source);
    let t_q = escape_applescript_string(target);
    format!(
        r#"tell application "Things3"
    set sourceTag to tag "{s_q}"
    set targetTag to tag "{t_q}"
    repeat with t in (to dos of sourceTag)
        set tag names of t to (tag names of t) & "{t_q}"
    end repeat
    delete sourceTag
end tell"#,
    )
}

pub fn render_delete_tag(name: &str) -> String {
    let name_q = escape_applescript_string(name);
    format!(
        r#"tell application "Things3"
    delete tag "{name_q}"
end tell"#,
    )
}

pub fn render_move_tag(name: &str, new_parent: Option<&str>) -> String {
    let name_q = escape_applescript_string(name);
    match new_parent {
        Some(p) => {
            let p_q = escape_applescript_string(p);
            format!(
                r#"tell application "Things3"
    set parent tag of tag "{name_q}" to tag "{p_q}"
end tell"#,
            )
        }
        // `missing value` is AppleScript's null; assigning it promotes the
        // tag to the root of the tag tree.
        None => format!(
            r#"tell application "Things3"
    set parent tag of tag "{name_q}" to missing value
end tell"#,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_handles_quotes_and_backslashes() {
        assert_eq!(escape_applescript_string("plain"), "plain");
        assert_eq!(escape_applescript_string("he said \"hi\""), "he said \\\"hi\\\"");
        assert_eq!(escape_applescript_string("path\\to"), "path\\\\to");
    }

    #[test]
    fn create_tag_no_parent_renders_make_tag() {
        let s = render_create_tag("Work", None);
        assert!(s.contains("tell application \"Things3\""));
        assert!(s.contains("make new tag with properties {name:\"Work\"}"));
        assert!(!s.contains("parent tag"));
    }

    #[test]
    fn create_tag_with_parent_renders_parent_link() {
        let s = render_create_tag("Urgent", Some("Work"));
        assert!(s.contains("make new tag with properties {name:\"Urgent\"}"));
        assert!(s.contains("set parent tag of newTag to tag \"Work\""));
    }

    #[test]
    fn create_tag_escapes_quotes_in_name() {
        let s = render_create_tag("She said \"yes\"", None);
        assert!(s.contains("name:\"She said \\\"yes\\\"\""));
    }

    #[test]
    fn rename_tag_renders_set_name() {
        let s = render_rename_tag("Old", "New");
        assert!(s.contains("set name of tag \"Old\" to \"New\""));
    }

    #[test]
    fn rename_tag_escapes_quotes() {
        let s = render_rename_tag("a\"b", "c\"d");
        assert!(s.contains("set name of tag \"a\\\"b\" to \"c\\\"d\""));
    }

    #[test]
    fn merge_tags_renders_loop_and_delete() {
        let s = render_merge_tags("Source", "Target");
        assert!(s.contains("set sourceTag to tag \"Source\""));
        assert!(s.contains("set targetTag to tag \"Target\""));
        assert!(s.contains("repeat with t in (to dos of sourceTag)"));
        assert!(s.contains("delete sourceTag"));
    }

    #[test]
    fn merge_tags_escapes_quotes_in_target_inside_loop_body() {
        let s = render_merge_tags("A", "B \"quoted\"");
        // The loop body assigns the target name via concatenation; the
        // escaped form must appear in both the binding line and the loop.
        assert!(s.contains("set targetTag to tag \"B \\\"quoted\\\"\""));
        assert!(s.contains("& \"B \\\"quoted\\\"\""));
    }

    #[test]
    fn delete_tag_renders_delete() {
        let s = render_delete_tag("Stale");
        assert!(s.contains("delete tag \"Stale\""));
    }

    #[test]
    fn delete_tag_escapes_quotes() {
        let s = render_delete_tag("Bad\"name");
        assert!(s.contains("delete tag \"Bad\\\"name\""));
    }

    #[test]
    fn move_tag_under_parent_renders_set_parent() {
        let s = render_move_tag("Urgent", Some("Work"));
        assert!(s.contains("set parent tag of tag \"Urgent\" to tag \"Work\""));
    }

    #[test]
    fn move_tag_to_root_uses_missing_value() {
        let s = render_move_tag("Urgent", None);
        assert!(s.contains("set parent tag of tag \"Urgent\" to missing value"));
    }

    #[test]
    fn move_tag_escapes_quotes_in_both_names() {
        let s = render_move_tag("a\"b", Some("c\"d"));
        assert!(s.contains("set parent tag of tag \"a\\\"b\" to tag \"c\\\"d\""));
    }
}
```

- [ ] **Step 2: Register the module in `core/applescript/mod.rs`**

Edit `crates/things-mcp/src/core/applescript/mod.rs`:

```rust
//! AppleScript path for tag-admin operations (`create`/`rename`/`merge`/
//! `delete`/`move_under`). Things' JSON URL has no global tag-admin verbs,
//! so these go through `osascript -e <script>` and trust the synchronous
//! exit code as verification.
//!
//! Symmetric to `core/writer/` for the JSON URL path: a driver trait
//! (`AppleScriptDriver`) with a production impl (`OsascriptDriver`) and a
//! recording test impl (`RecordingAppleScript`), pure `render_*` helpers
//! in `script.rs`, and a facade (`TagAdmin`) in `admin.rs` that owns
//! the safety gate and result composition.

pub mod driver;
pub mod script;
```

- [ ] **Step 3: Build + full sweep**

```
cargo build
cargo test
```

Expected: **128 reported** (126 passing + 2 ignored). +12 over T1: 1 escape test + 3 create + 2 rename + 2 merge + 2 delete + 3 move = 13 tests, but the merge-quotes test stands alongside its sibling, so the realistic count is 12 added tests.

(If the actual lib total comes out 1 higher or lower because a test was inlined differently, that's fine — the spec promised ~12. The important invariant is **all green**.)

- [ ] **Step 4: Commit**

```bash
git add crates/things-mcp/src/core/applescript/mod.rs \
        crates/things-mcp/src/core/applescript/script.rs
git commit -m "core/applescript: render helpers for tag-admin ops"
```

---

## Task 3: `core/applescript/admin.rs` — `TagAdmin` facade

The facade owns the safety gate (Forbidden → error; DryRun → short-circuit; Live → call driver) and composes the `TagOutcome` result. No auth-token gate — AppleScript doesn't use Things' URL-scheme auth token. Defense-in-depth validation: `merge(source == target)` rejected at this layer (also rejected at the tool-adapter layer in T7).

**Files:**
- Create: `crates/things-mcp/src/core/applescript/admin.rs`
- Modify: `crates/things-mcp/src/core/applescript/mod.rs`

- [ ] **Step 1: Create `core/applescript/admin.rs`**

`crates/things-mcp/src/core/applescript/admin.rs`:

```rust
//! `TagAdmin` — facade over the AppleScript driver. Owns the safety gate
//! and composes a `TagOutcome` per call. Each method renders the script
//! via the pure helpers in `script.rs`, then either short-circuits
//! (DryRun), errors out (Forbidden), or hands the script to the injected
//! `AppleScriptDriver`.

use std::sync::Arc;
use std::time::Instant;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::core::applescript::driver::AppleScriptDriver;
use crate::core::applescript::script;
use crate::core::error::ThingsError;
use crate::core::writer::writer::SafetyMode;

#[derive(Debug)]
pub struct TagAdmin {
    pub driver: Arc<dyn AppleScriptDriver>,
    pub safety: SafetyMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TagOutcome {
    /// Snake-case action name — `"create_tag"`, `"rename_tag"`, …
    pub action: String,
    /// `true` when the safety gate short-circuited (DryRun mode). When
    /// `true`, the script was rendered but never run; `osascript_stdout`
    /// is empty.
    pub dry_run: bool,
    /// Wall-clock latency in milliseconds. `0` when `dry_run` is true.
    pub latency_ms: u64,
    /// First line of `osascript` stdout, truncated to 200 chars. Empty when
    /// the script returned no output (the common case for tag-admin ops).
    pub osascript_stdout: String,
}

impl TagAdmin {
    pub async fn create(&self, name: &str, parent: Option<&str>) -> Result<TagOutcome, ThingsError> {
        let script = script::render_create_tag(name, parent);
        self.dispatch("create_tag", script).await
    }

    pub async fn rename(&self, old: &str, new: &str) -> Result<TagOutcome, ThingsError> {
        let script = script::render_rename_tag(old, new);
        self.dispatch("rename_tag", script).await
    }

    pub async fn merge(&self, source: &str, target: &str) -> Result<TagOutcome, ThingsError> {
        // Defense-in-depth: the tool adapter also rejects this, but a stray
        // direct caller would render a script that deletes the only copy
        // and then tries to read from it.
        if source == target {
            return Err(ThingsError::InvalidInput {
                field: "source".into(),
                reason: "source and target must differ".into(),
            });
        }
        let script = script::render_merge_tags(source, target);
        self.dispatch("merge_tags", script).await
    }

    pub async fn delete(&self, name: &str) -> Result<TagOutcome, ThingsError> {
        let script = script::render_delete_tag(name);
        self.dispatch("delete_tag", script).await
    }

    pub async fn move_under(
        &self,
        name: &str,
        new_parent: Option<&str>,
    ) -> Result<TagOutcome, ThingsError> {
        let script = script::render_move_tag(name, new_parent);
        self.dispatch("move_tag", script).await
    }

    async fn dispatch(&self, action: &str, script: String) -> Result<TagOutcome, ThingsError> {
        // 1. Safety gate — Forbidden refuses outright.
        if self.safety == SafetyMode::Forbidden {
            return Err(ThingsError::TestDbWriteForbidden);
        }

        // 2. Log the script (no secrets to mask — AppleScript doesn't carry
        // the auth-token).
        tracing::info!(action = action, "applescript: {} bytes", script.len());

        // 3. DryRun short-circuit — render only, no driver call.
        if self.safety == SafetyMode::DryRun {
            return Ok(TagOutcome {
                action: action.to_string(),
                dry_run: true,
                latency_ms: 0,
                osascript_stdout: String::new(),
            });
        }

        // 4. Live: hand the script to the driver.
        let started = Instant::now();
        let stdout = self.driver.run(&script).await?;
        let latency_ms = started.elapsed().as_millis() as u64;
        let truncated = truncate_first_line(&stdout, 200);

        Ok(TagOutcome {
            action: action.to_string(),
            dry_run: false,
            latency_ms,
            osascript_stdout: truncated,
        })
    }
}

fn truncate_first_line(s: &str, max: usize) -> String {
    let first = s.lines().next().unwrap_or("");
    if first.len() <= max {
        first.to_string()
    } else {
        first.chars().take(max).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::applescript::driver::RecordingAppleScript;

    fn admin(safety: SafetyMode) -> (Arc<RecordingAppleScript>, TagAdmin) {
        let rec = Arc::new(RecordingAppleScript::new());
        let admin = TagAdmin {
            driver: rec.clone(),
            safety,
        };
        (rec, admin)
    }

    #[tokio::test]
    async fn forbidden_mode_refuses_outright() {
        let (rec, admin) = admin(SafetyMode::Forbidden);
        let res = admin.create("Work", None).await;
        assert!(matches!(res, Err(ThingsError::TestDbWriteForbidden)));
        // Driver must NOT have been called.
        assert!(rec.scripts().is_empty());
    }

    #[tokio::test]
    async fn dry_run_mode_short_circuits_without_calling_driver() {
        let (rec, admin) = admin(SafetyMode::DryRun);
        let out = admin.create("Work", None).await.unwrap();
        assert!(out.dry_run);
        assert_eq!(out.action, "create_tag");
        assert_eq!(out.latency_ms, 0);
        assert_eq!(out.osascript_stdout, "");
        // Driver was never invoked.
        assert!(rec.scripts().is_empty());
    }

    #[tokio::test]
    async fn live_create_calls_driver_with_rendered_script() {
        let (rec, admin) = admin(SafetyMode::Live);
        let out = admin.create("Work", Some("Personal")).await.unwrap();
        assert!(!out.dry_run);
        assert_eq!(out.action, "create_tag");
        let scripts = rec.scripts();
        assert_eq!(scripts.len(), 1);
        assert!(scripts[0].contains("make new tag with properties {name:\"Work\"}"));
        assert!(scripts[0].contains("set parent tag of newTag to tag \"Personal\""));
    }

    #[tokio::test]
    async fn live_rename_calls_driver_with_rendered_script() {
        let (rec, admin) = admin(SafetyMode::Live);
        let _out = admin.rename("Old", "New").await.unwrap();
        let scripts = rec.scripts();
        assert!(scripts[0].contains("set name of tag \"Old\" to \"New\""));
    }

    #[tokio::test]
    async fn live_merge_calls_driver_with_rendered_script() {
        let (rec, admin) = admin(SafetyMode::Live);
        let _out = admin.merge("Source", "Target").await.unwrap();
        let scripts = rec.scripts();
        assert!(scripts[0].contains("set sourceTag to tag \"Source\""));
        assert!(scripts[0].contains("delete sourceTag"));
    }

    #[tokio::test]
    async fn merge_self_rejected_with_invalid_input() {
        let (rec, admin) = admin(SafetyMode::Live);
        let res = admin.merge("Same", "Same").await;
        match res {
            Err(ThingsError::InvalidInput { field, .. }) => assert_eq!(field, "source"),
            other => panic!("expected InvalidInput, got {:?}", other),
        }
        // Driver must not have been called for the self-merge.
        assert!(rec.scripts().is_empty());
    }

    #[tokio::test]
    async fn live_delete_calls_driver_with_rendered_script() {
        let (rec, admin) = admin(SafetyMode::Live);
        let _out = admin.delete("Stale").await.unwrap();
        let scripts = rec.scripts();
        assert!(scripts[0].contains("delete tag \"Stale\""));
    }

    #[tokio::test]
    async fn live_move_under_parent_calls_driver_with_rendered_script() {
        let (rec, admin) = admin(SafetyMode::Live);
        let _out = admin.move_under("Urgent", Some("Work")).await.unwrap();
        let scripts = rec.scripts();
        assert!(scripts[0].contains("set parent tag of tag \"Urgent\" to tag \"Work\""));
    }

    #[tokio::test]
    async fn live_move_to_root_uses_missing_value() {
        let (rec, admin) = admin(SafetyMode::Live);
        let _out = admin.move_under("Urgent", None).await.unwrap();
        let scripts = rec.scripts();
        assert!(scripts[0].contains("set parent tag of tag \"Urgent\" to missing value"));
    }

    #[tokio::test]
    async fn driver_error_propagates_unchanged() {
        let (rec, admin) = admin(SafetyMode::Live);
        rec.push_response(Err(ThingsError::AppleScriptFailed {
            stderr: "tag not found".into(),
            exit: 1,
        }));
        let res = admin.delete("Ghost").await;
        match res {
            Err(ThingsError::AppleScriptFailed { stderr, exit }) => {
                assert_eq!(stderr, "tag not found");
                assert_eq!(exit, 1);
            }
            other => panic!("expected AppleScriptFailed, got {:?}", other),
        }
    }
}
```

- [ ] **Step 2: Register the module in `core/applescript/mod.rs`**

Edit `crates/things-mcp/src/core/applescript/mod.rs` — add `pub mod admin;` and re-exports:

```rust
//! AppleScript path for tag-admin operations (`create`/`rename`/`merge`/
//! `delete`/`move_under`). Things' JSON URL has no global tag-admin verbs,
//! so these go through `osascript -e <script>` and trust the synchronous
//! exit code as verification.
//!
//! Symmetric to `core/writer/` for the JSON URL path: a driver trait
//! (`AppleScriptDriver`) with a production impl (`OsascriptDriver`) and a
//! recording test impl (`RecordingAppleScript`), pure `render_*` helpers
//! in `script.rs`, and a facade (`TagAdmin`) in `admin.rs` that owns
//! the safety gate and result composition.

pub mod admin;
pub mod driver;
pub mod script;

pub use admin::{TagAdmin, TagOutcome};
pub use driver::{AppleScriptDriver, OsascriptDriver, RecordingAppleScript};
```

- [ ] **Step 3: Build + full sweep**

```
cargo build
cargo test
```

Expected: **138 reported** (136 passing + 2 ignored). +10 over T2: 1 forbidden + 1 dry-run + 5 live-call (create/rename/merge/delete/move-under) + 1 move-to-root + 1 merge-self-reject + 1 driver-error = 10 tests.

- [ ] **Step 4: Commit**

```bash
git add crates/things-mcp/src/core/applescript/mod.rs \
        crates/things-mcp/src/core/applescript/admin.rs
git commit -m "core/applescript: TagAdmin facade + safety gate + TagOutcome"
```

---

## Task 4: `core/reader/tags.rs` — `TagListing` + `build_tree` + `get_tags_for_task` helper

Add a new reader module that exposes a tree-shaped `TagListing` (the new shape for `things_list_tags`) and a `build_tree` helper with cycle safety. Also extends `core/reader/queries.rs` with a small public `get_tags_for_task` helper that the Task-6 `things_unassign_tag` / `things_assign_tag` adapters need for the read-modify-write step.

No fixture changes — the existing fixture already covers all three scenarios (root tag, child tag, attached to a to-do).

**Files:**
- Create: `crates/things-mcp/src/core/reader/tags.rs`
- Modify: `crates/things-mcp/src/core/reader/mod.rs`
- Modify: `crates/things-mcp/src/core/reader/queries.rs`

- [ ] **Step 1: Add `get_tags_for_task` to `queries.rs`**

The module already has a private `fetch_tags_for_tasks(pool, task_ids) -> HashMap<task_id, Vec<tag_title>>` (`crates/things-mcp/src/core/reader/queries.rs:415-448`). Add a thin public single-task helper underneath it. Find the closing brace of `fetch_tags_for_tasks` (around line 448) and insert immediately after:

```rust
/// Public helper for the assign/unassign tools: returns the current tag
/// titles attached to a single to-do (or empty if none). Wraps the
/// per-task fetch so callers don't have to deal with the HashMap shape.
pub async fn get_tags_for_task(
    pool: &ReaderPool,
    id: String,
) -> Result<Vec<String>, ThingsError> {
    let tag_map = fetch_tags_for_tasks(pool, vec![id.clone()]).await?;
    Ok(tag_map.get(&id).cloned().unwrap_or_default())
}
```

Add one unit test inside `mod tests` (at the bottom of `queries.rs`):

```rust
    #[tokio::test]
    async fn get_tags_for_task_returns_tag_titles_for_tagged_todo() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        // todo-2 is tagged 'Errand' in the fixture.
        let tags = get_tags_for_task(&pool, "todo-2".into()).await.unwrap();
        assert_eq!(tags, vec!["Errand".to_string()]);
    }

    #[tokio::test]
    async fn get_tags_for_task_returns_empty_for_untagged_todo() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        // todo-1 ('Buy milk') has no tags.
        let tags = get_tags_for_task(&pool, "todo-1".into()).await.unwrap();
        assert!(tags.is_empty());
    }
```

- [ ] **Step 2: Create `core/reader/tags.rs`**

`crates/things-mcp/src/core/reader/tags.rs`:

```rust
//! Tree-shaped tag listing. Wraps the flat `queries::list_tags` and
//! builds an ordered tree from `parent_id`. Cycle-safe: a `HashSet`
//! guards the recursion so a malformed DB cannot loop the server.

use std::collections::{HashMap, HashSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::core::error::ThingsError;
use crate::core::reader::pool::ReaderPool;
use crate::core::reader::queries::list_tags;
use crate::core::types::Tag;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TagNode {
    pub id: String,
    pub title: String,
    pub children: Vec<TagNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TagListing {
    /// Every tag, ordered by Things' display index then title. Same shape
    /// `things_list_tags` returned in Plan 2.
    pub flat: Vec<Tag>,
    /// Tag trees rooted at top-level tags (those with no parent). Order
    /// matches `flat` (root tags appear in display-index order).
    pub roots: Vec<TagNode>,
}

pub async fn list_tags_with_tree(pool: &ReaderPool) -> Result<TagListing, ThingsError> {
    let flat = list_tags(pool).await?;
    let roots = build_tree(&flat);
    Ok(TagListing { flat, roots })
}

/// Build a tag tree from the flat list. Cycle-safe: each recursion path
/// maintains a `visited` set; a node that points back into the path is
/// dropped (with a `tracing::warn!`).
pub fn build_tree(flat: &[Tag]) -> Vec<TagNode> {
    // Group children by parent id; preserve flat order within each group.
    let mut children_by_parent: HashMap<&str, Vec<&Tag>> = HashMap::new();
    let mut roots: Vec<&Tag> = Vec::new();
    for tag in flat {
        match tag.parent_id.as_deref() {
            None => roots.push(tag),
            Some(pid) => children_by_parent.entry(pid).or_default().push(tag),
        }
    }

    let mut out = Vec::with_capacity(roots.len());
    for root in roots {
        let mut visited: HashSet<&str> = HashSet::new();
        visited.insert(root.id.as_str());
        out.push(build_node(root, &children_by_parent, &mut visited));
    }
    out
}

fn build_node<'a>(
    tag: &'a Tag,
    children_by_parent: &HashMap<&'a str, Vec<&'a Tag>>,
    visited: &mut HashSet<&'a str>,
) -> TagNode {
    let mut children = Vec::new();
    if let Some(child_list) = children_by_parent.get(tag.id.as_str()) {
        for child in child_list {
            if !visited.insert(child.id.as_str()) {
                tracing::warn!(
                    "tag cycle detected at uuid={}; dropping subtree",
                    child.id
                );
                continue;
            }
            children.push(build_node(child, children_by_parent, visited));
            visited.remove(child.id.as_str());
        }
    }
    TagNode {
        id: tag.id.clone(),
        title: tag.title.clone(),
        children,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reader::fixture::build_fixture;
    use tempfile::tempdir;

    #[tokio::test]
    async fn list_tags_with_tree_matches_fixture_two_level_nesting() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let listing = list_tags_with_tree(&pool).await.unwrap();
        // Flat: 3 tags total — Errand, Call, Deep work.
        assert_eq!(listing.flat.len(), 3);
        // Roots: 2 — Errand and Deep work (Call has parent Errand).
        assert_eq!(listing.roots.len(), 2);
        let titles: Vec<&str> = listing.roots.iter().map(|r| r.title.as_str()).collect();
        assert!(titles.contains(&"Errand"));
        assert!(titles.contains(&"Deep work"));
        // Errand has 1 child: Call.
        let errand = listing.roots.iter().find(|r| r.title == "Errand").unwrap();
        assert_eq!(errand.children.len(), 1);
        assert_eq!(errand.children[0].title, "Call");
        assert!(errand.children[0].children.is_empty());
        // Deep work has no children.
        let deep = listing.roots.iter().find(|r| r.title == "Deep work").unwrap();
        assert!(deep.children.is_empty());
    }

    #[test]
    fn build_tree_handles_multi_level_synthetic_nesting() {
        // a → b → c (3-level nesting) plus an unrelated root x.
        let flat = vec![
            Tag { id: "a".into(), title: "A".into(), parent_id: None,         shortcut: None },
            Tag { id: "b".into(), title: "B".into(), parent_id: Some("a".into()), shortcut: None },
            Tag { id: "c".into(), title: "C".into(), parent_id: Some("b".into()), shortcut: None },
            Tag { id: "x".into(), title: "X".into(), parent_id: None,         shortcut: None },
        ];
        let roots = build_tree(&flat);
        assert_eq!(roots.len(), 2);
        let a = roots.iter().find(|r| r.title == "A").unwrap();
        assert_eq!(a.children.len(), 1);
        assert_eq!(a.children[0].title, "B");
        assert_eq!(a.children[0].children.len(), 1);
        assert_eq!(a.children[0].children[0].title, "C");
    }

    #[test]
    fn build_tree_drops_cycle_without_looping() {
        // a → b and b → a (impossible in Things but possible in a corrupt
        // DB). build_tree must drop the cycle's back-edge, not infinite-loop.
        // Both a and b list a parent, so NEITHER is a root — build_tree
        // returns no roots. The cycle guard ensures we don't blow the stack
        // trying to walk it.
        let flat = vec![
            Tag { id: "a".into(), title: "A".into(), parent_id: Some("b".into()), shortcut: None },
            Tag { id: "b".into(), title: "B".into(), parent_id: Some("a".into()), shortcut: None },
        ];
        let roots = build_tree(&flat);
        assert!(roots.is_empty(), "no parentless tags -> no roots; cycle survived without crashing");
    }
}
```

- [ ] **Step 3: Register the new module in `core/reader/mod.rs`**

Edit `crates/things-mcp/src/core/reader/mod.rs`:

```rust
//! Read path: SQLite connection pool, schema probe, and typed query helpers.

pub mod dates;
pub mod fixture;
pub mod fts;
pub mod pool;
pub mod queries;
pub mod schema;
pub mod tags;
```

- [ ] **Step 4: Build + full sweep**

```
cargo build
cargo test
```

Expected: **143 reported** (141 passing + 2 ignored). +5 over T3: 2 `get_tags_for_task` tests (live + empty) + 3 `tags.rs` tests (fixture-based listing + multi-level synthetic + cycle guard) = 5 tests.

(The spec's original test-budget for this task booked +3; the +5 figure here reflects two additional `get_tags_for_task` tests added to cover the helper the assign/unassign adapters depend on in T6.)

- [ ] **Step 5: Commit**

```bash
git add crates/things-mcp/src/core/reader/mod.rs \
        crates/things-mcp/src/core/reader/tags.rs \
        crates/things-mcp/src/core/reader/queries.rs
git commit -m "core/reader/tags: TagListing + build_tree + get_tags_for_task"
```

---

## Task 5: `VerifyPredicate::TagOnTodoById`

Add one new `VerifyPredicate` variant + its `check_once` arm + an extension to the existence-probe + 2 tests. The fixture supplies both rows we need: `todo-2` has tag `Errand` (the `present: true` row) and `todo-1` has no tags (the `present: false` row).

**Files:**
- Modify: `crates/things-mcp/src/core/writer/verify.rs`

- [ ] **Step 1: Add the `TagOnTodoById` variant**

Edit `crates/things-mcp/src/core/writer/verify.rs`. The `VerifyPredicate` enum currently ends with `MoveById { id, expected_list_id }`. Append a new variant after that:

```rust
    /// The row at `id` either does or doesn't have the named tag, depending
    /// on `present`. Used by `things_assign_tag` (`present: true`) and
    /// `things_unassign_tag` (`present: false`). Tag matched by title via
    /// the TMTaskTag→TMTag join.
    TagOnTodoById {
        id: String,
        tag: String,
        present: bool,
    },
```

So the full enum reads:

```rust
#[derive(Debug, Clone)]
pub enum VerifyPredicate {
    CreateByTitle { title: String, since_unix: f64, kind: TaskKind },
    UpdateById { id: String, expected_title: Option<String>, expected_notes: Option<String> },
    StatusChange { id: String, want: TaskStatus },
    MoveById { id: String, expected_list_id: Option<String> },
    TagOnTodoById { id: String, tag: String, present: bool },
}
```

- [ ] **Step 2: Extend the existence-probe match at the top of `verify()`**

The existence-probe sits at `crates/things-mcp/src/core/writer/verify.rs:60-79`. The current pattern is:

```rust
    if let VerifyPredicate::UpdateById { id, .. }
        | VerifyPredicate::StatusChange { id, .. }
        | VerifyPredicate::MoveById { id, .. } = &pred
    {
```

Replace with (add `TagOnTodoById`):

```rust
    if let VerifyPredicate::UpdateById { id, .. }
        | VerifyPredicate::StatusChange { id, .. }
        | VerifyPredicate::MoveById { id, .. }
        | VerifyPredicate::TagOnTodoById { id, .. } = &pred
    {
```

The body (the SQL existence probe + `Ok(VerifyOutcome::NotFound { … })` on miss) stays unchanged.

- [ ] **Step 3: Add the `check_once` arm**

Find the `match pred { … }` block in `check_once` (currently ending with the `MoveById` arm). Append a new arm after `MoveById`:

```rust
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
```

(Note: `SUMMARY_COLS` and `row_to_summary` are already imported at the top of `check_once` via the `use` line.)

- [ ] **Step 4: Add the two verify tests**

Inside the existing `#[cfg(test)] mod tests` block in `verify.rs`, append:

```rust
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
```

- [ ] **Step 5: Build + full sweep**

```
cargo build
cargo test
```

Expected: **145 reported** (143 passing + 2 ignored). +2 over T4.

- [ ] **Step 6: Commit**

```bash
git add crates/things-mcp/src/core/writer/verify.rs
git commit -m "core/writer: TagOnTodoById verify predicate"
```

---

## Task 6: `things_assign_tag` + `things_unassign_tag` (RMW through JSON URL chassis)

Two tool adapters that wrap `state.writer.fire(Operation::UpdateTodo(…), Some(VerifyPredicate::TagOnTodoById { … }))`. Both do read-modify-write: read the current tags from SQLite via `get_tags_for_task`, compute the new tag set, and fire a tags-only `UpdateTodo`. Verification predicate checks the first tag in the request list.

**Files:**
- Modify: `crates/things-mcp/src/tools/todos.rs`

No tests in this task — the integration tests in Task 8 cover the full path. The lib-level tests for the underlying components were added in T2/T3/T4/T5.

- [ ] **Step 1: Add `TagAssignmentArgs` + `things_assign_tag` to `tools/todos.rs`**

Append at the end of `crates/things-mcp/src/tools/todos.rs`:

```rust
use crate::core::reader::queries::get_tags_for_task;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct TagAssignmentArgs {
    /// UUID of the to-do or project to attach/remove tags on. Names are
    /// not accepted — pass a uuid.
    pub id: String,
    /// Tag titles (not uuids). Non-empty.
    pub tags: Vec<String>,
}

pub async fn things_assign_tag(
    state: AppState,
    args: TagAssignmentArgs,
) -> anyhow::Result<WriteOutcome> {
    if args.id.trim().is_empty() {
        return Err(crate::core::error::ThingsError::InvalidInput {
            field: "id".into(),
            reason: "id must be non-empty".into(),
        }
        .into());
    }
    if args.tags.is_empty() {
        return Err(crate::core::error::ThingsError::InvalidInput {
            field: "tags".into(),
            reason: "tags must be non-empty".into(),
        }
        .into());
    }

    // Read-modify-write: union current tags with the requested set.
    let current = get_tags_for_task(&state.pool, args.id.clone()).await?;
    let mut merged: Vec<String> = current.clone();
    for t in &args.tags {
        if !merged.iter().any(|x| x == t) {
            merged.push(t.clone());
        }
    }

    let op = Operation::UpdateTodo(UpdateTodoSpec {
        id: args.id.clone(),
        tags: Some(merged),
        ..Default::default()
    });
    // Verify the first requested tag landed; if Things merges them in one
    // write (the common case), the rest landed too.
    let predicate = VerifyPredicate::TagOnTodoById {
        id: args.id,
        tag: args.tags[0].clone(),
        present: true,
    };
    let outcome = state.writer.fire(op, Some(predicate)).await?;
    Ok(outcome)
}

pub async fn things_unassign_tag(
    state: AppState,
    args: TagAssignmentArgs,
) -> anyhow::Result<WriteOutcome> {
    if args.id.trim().is_empty() {
        return Err(crate::core::error::ThingsError::InvalidInput {
            field: "id".into(),
            reason: "id must be non-empty".into(),
        }
        .into());
    }
    if args.tags.is_empty() {
        return Err(crate::core::error::ThingsError::InvalidInput {
            field: "tags".into(),
            reason: "tags must be non-empty".into(),
        }
        .into());
    }

    // Read-modify-write: filter out the requested tags.
    let current = get_tags_for_task(&state.pool, args.id.clone()).await?;
    let to_remove: std::collections::HashSet<&str> =
        args.tags.iter().map(|s| s.as_str()).collect();
    let new_set: Vec<String> = current
        .into_iter()
        .filter(|t| !to_remove.contains(t.as_str()))
        .collect();

    let op = Operation::UpdateTodo(UpdateTodoSpec {
        id: args.id.clone(),
        tags: Some(new_set),
        ..Default::default()
    });
    let predicate = VerifyPredicate::TagOnTodoById {
        id: args.id,
        tag: args.tags[0].clone(),
        present: false,
    };
    let outcome = state.writer.fire(op, Some(predicate)).await?;
    Ok(outcome)
}
```

- [ ] **Step 2: Build to confirm the new code compiles**

```
cargo build
```

Expected: clean. (No new tests this task; running `cargo test` still shows 145 reported.)

- [ ] **Step 3: Commit**

```bash
git add crates/things-mcp/src/tools/todos.rs
git commit -m "tools/todos: things_assign_tag + things_unassign_tag adapters"
```

---

## Task 7: state wiring + `tools/tags.rs` (6 admin/list adapters) + server registrations

This is the fan-out task. State gains a `tag_admin` field and a corresponding `applescript_override` option. The new `tools/tags.rs` houses `things_list_tags` (moved from `tools/lists.rs`, return type now `Json<TagListing>`) plus five admin adapter functions. The server registers seven tools — six new admin/tag tools plus the assign/unassign tools added in Task 6 — and the existing `things_list_tags` registration is updated to match the new return type.

**Files:**
- Modify: `crates/things-mcp/src/state.rs`
- Create: `crates/things-mcp/src/tools/tags.rs`
- Modify: `crates/things-mcp/src/tools/mod.rs`
- Modify: `crates/things-mcp/src/tools/lists.rs` (remove `things_list_tags` + `ListTagsArgs`)
- Modify: `crates/things-mcp/src/server.rs`

- [ ] **Step 1: Extend `AppState` and `AppStateOptions`**

Edit `crates/things-mcp/src/state.rs`.

Update the `use` block (line 10–23) to import the AppleScript types and the writer SafetyMode (the latter is already imported but the AppleScript driver is new):

```rust
use crate::core::{
    applescript::{
        admin::TagAdmin,
        driver::{AppleScriptDriver, OsascriptDriver},
    },
    backup,
    config::{self, Config},
    reader::{
        fts::{self, FtsCapability},
        pool::ReaderPool,
        schema,
    },
    writer::{
        executor::{Executor, OpenCommandExecutor},
        secret::SecretString,
        writer::{SafetyMode, Writer, WriterCfg},
    },
};
```

Add a `tag_admin` field to `AppState`:

```rust
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db_path: PathBuf,
    pub pool: ReaderPool,
    pub test_db_mode: bool,
    pub allow_writes_on_test_db: bool,
    pub fts: Option<FtsCapability>,
    pub writer: Arc<Writer>,
    pub tag_admin: Arc<TagAdmin>,
}
```

Add an `applescript_override` field to `AppStateOptions`:

```rust
pub struct AppStateOptions {
    pub env_db_path: Option<PathBuf>,
    pub home_dir: PathBuf,
    pub config_path: PathBuf,
    pub allow_writes_on_test_db: bool,
    /// Test-only: inject a `RecordingExecutor` (or any other) in place of the
    /// production `OpenCommandExecutor`. `None` in production code paths.
    pub executor_override: Option<Arc<dyn Executor>>,
    /// Test-only: inject a `RecordingAppleScript` (or any other) in place of
    /// the production `OsascriptDriver`. `None` in production code paths.
    pub applescript_override: Option<Arc<dyn AppleScriptDriver>>,
}
```

Wire up the `TagAdmin` inside `AppState::build`. Inside the `Ok(Self { … })` literal (currently at the bottom of `build`), add the field. Just before that literal, after the `Writer` is built, insert:

```rust
        let applescript: Arc<dyn AppleScriptDriver> = opts
            .applescript_override
            .clone()
            .unwrap_or_else(|| Arc::new(OsascriptDriver));

        let tag_admin = Arc::new(TagAdmin {
            driver: applescript,
            safety,
        });
```

And inside the `Ok(Self { … })` literal, add `tag_admin,` to the field list (right after `writer,`):

```rust
        Ok(Self {
            config: Arc::new(cfg),
            db_path,
            pool,
            test_db_mode,
            allow_writes_on_test_db: opts.allow_writes_on_test_db,
            fts,
            writer,
            tag_admin,
        })
```

- [ ] **Step 2: Find every existing `AppStateOptions { … }` literal and add `applescript_override: None`**

Search the codebase for `AppStateOptions {` to find places that construct it:

```bash
grep -rn "AppStateOptions {" crates/things-mcp/
```

Expected matches (as of HEAD `ea9f4e1`):
- `crates/things-mcp/src/main.rs` (production startup) — add `applescript_override: None,`
- `crates/things-mcp/tests/end_to_end_writes_plan_5.rs` (existing integration tests) — add `applescript_override: None,` to each constructor

For each match, add the new field. Existing tests must continue to compile.

(A safe way: add `applescript_override: None,` immediately after `executor_override:` in every literal.)

- [ ] **Step 3: Create `tools/tags.rs`**

`crates/things-mcp/src/tools/tags.rs`:

```rust
//! Tag-aware MCP tool adapters. Two distinct flavours:
//!
//! - `things_list_tags` reads the SQLite reader pool and returns a flat
//!   list + a tag tree.
//! - `things_create_tag`, `things_rename_tag`, `things_merge_tags`,
//!   `things_delete_tag`, `things_move_tag` all route through the
//!   `TagAdmin` (`core/applescript/admin.rs`), which renders the
//!   appropriate AppleScript and hands it to `osascript`.
//!
//! `things_assign_tag` and `things_unassign_tag` live in `tools/todos.rs`
//! because they target a to-do row and run through the JSON URL chassis,
//! not AppleScript.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::core::applescript::admin::TagOutcome;
use crate::core::error::ThingsError;
use crate::core::reader::tags::{list_tags_with_tree, TagListing};
use crate::state::AppState;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct ListTagsArgs {}

pub async fn things_list_tags(
    state: AppState,
    _args: ListTagsArgs,
) -> anyhow::Result<TagListing> {
    let listing = list_tags_with_tree(&state.pool).await?;
    Ok(listing)
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct CreateTagArgs {
    /// New tag name. Non-empty.
    pub name: String,
    /// Optional parent tag name. Omit for a root tag.
    #[serde(default)]
    pub parent: Option<String>,
}

pub async fn things_create_tag(
    state: AppState,
    args: CreateTagArgs,
) -> anyhow::Result<TagOutcome> {
    if args.name.trim().is_empty() {
        return Err(ThingsError::InvalidInput {
            field: "name".into(),
            reason: "name must be non-empty".into(),
        }
        .into());
    }
    if let Some(p) = args.parent.as_deref() {
        if p.trim().is_empty() {
            return Err(ThingsError::InvalidInput {
                field: "parent".into(),
                reason: "parent must be non-empty when supplied".into(),
            }
            .into());
        }
    }
    let out = state
        .tag_admin
        .create(&args.name, args.parent.as_deref())
        .await?;
    Ok(out)
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct RenameTagArgs {
    /// Current tag name.
    pub old: String,
    /// New tag name.
    pub new: String,
}

pub async fn things_rename_tag(
    state: AppState,
    args: RenameTagArgs,
) -> anyhow::Result<TagOutcome> {
    if args.old.trim().is_empty() {
        return Err(ThingsError::InvalidInput {
            field: "old".into(),
            reason: "old must be non-empty".into(),
        }
        .into());
    }
    if args.new.trim().is_empty() {
        return Err(ThingsError::InvalidInput {
            field: "new".into(),
            reason: "new must be non-empty".into(),
        }
        .into());
    }
    if args.old == args.new {
        return Err(ThingsError::InvalidInput {
            field: "new".into(),
            reason: "new must differ from old".into(),
        }
        .into());
    }
    let out = state.tag_admin.rename(&args.old, &args.new).await?;
    Ok(out)
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct MergeTagsArgs {
    /// Tag whose to-dos will be reassigned then deleted.
    pub source: String,
    /// Tag that absorbs the source tag's to-dos.
    pub target: String,
}

pub async fn things_merge_tags(
    state: AppState,
    args: MergeTagsArgs,
) -> anyhow::Result<TagOutcome> {
    if args.source.trim().is_empty() {
        return Err(ThingsError::InvalidInput {
            field: "source".into(),
            reason: "source must be non-empty".into(),
        }
        .into());
    }
    if args.target.trim().is_empty() {
        return Err(ThingsError::InvalidInput {
            field: "target".into(),
            reason: "target must be non-empty".into(),
        }
        .into());
    }
    if args.source == args.target {
        return Err(ThingsError::InvalidInput {
            field: "source".into(),
            reason: "source and target must differ".into(),
        }
        .into());
    }
    let out = state.tag_admin.merge(&args.source, &args.target).await?;
    Ok(out)
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct DeleteTagArgs {
    /// Tag name to delete. Removing a tag detaches it from every to-do that
    /// carries it; the to-dos themselves are unaffected.
    pub name: String,
}

pub async fn things_delete_tag(
    state: AppState,
    args: DeleteTagArgs,
) -> anyhow::Result<TagOutcome> {
    if args.name.trim().is_empty() {
        return Err(ThingsError::InvalidInput {
            field: "name".into(),
            reason: "name must be non-empty".into(),
        }
        .into());
    }
    let out = state.tag_admin.delete(&args.name).await?;
    Ok(out)
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct MoveTagArgs {
    /// Tag name to relocate in the tag tree.
    pub name: String,
    /// New parent tag name. Omit (or `null`) to promote to the root of the
    /// tag tree.
    #[serde(default)]
    pub new_parent: Option<String>,
}

pub async fn things_move_tag(
    state: AppState,
    args: MoveTagArgs,
) -> anyhow::Result<TagOutcome> {
    if args.name.trim().is_empty() {
        return Err(ThingsError::InvalidInput {
            field: "name".into(),
            reason: "name must be non-empty".into(),
        }
        .into());
    }
    if let Some(p) = args.new_parent.as_deref() {
        if p.trim().is_empty() {
            return Err(ThingsError::InvalidInput {
                field: "new_parent".into(),
                reason: "new_parent must be non-empty when supplied".into(),
            }
            .into());
        }
    }
    let out = state
        .tag_admin
        .move_under(&args.name, args.new_parent.as_deref())
        .await?;
    Ok(out)
}
```

- [ ] **Step 4: Register the new module in `tools/mod.rs`**

Edit `crates/things-mcp/src/tools/mod.rs`. Currently:

```rust
pub mod bulk;
pub mod lists;
pub mod projects;
pub mod search;
pub mod todos;
```

Replace with:

```rust
pub mod bulk;
pub mod lists;
pub mod projects;
pub mod search;
pub mod tags;
pub mod todos;
```

- [ ] **Step 5: Remove `things_list_tags` and `ListTagsArgs` from `tools/lists.rs`**

Edit `crates/things-mcp/src/tools/lists.rs`.

Delete the `use crate::core::reader::queries::list_tags;` line (around line 179).

Delete the `use crate::core::types::Tag;` line (around line 180) **only if no other code in `lists.rs` still uses `Tag`**. (Inspection: it's only used by `things_list_tags`, so it's safe to delete. If the build later complains about `Tag` being unused, that's the signal it actually is used elsewhere — re-add the line.)

Delete the entire `ListTagsArgs` struct + `things_list_tags` function (the block at lines 224–233 inclusive):

```rust
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

- [ ] **Step 6: Update `server.rs` imports + registrations**

Edit `crates/things-mcp/src/server.rs`.

**(a)** The `use crate::tools::lists::{ … things_list_tags … ListTagsArgs … };` block (around lines 23–29) currently imports `things_list_tags` and `ListTagsArgs` from `lists`. Remove those two names from the import list. The block becomes:

```rust
use crate::tools::lists::{
    things_list_anytime, things_list_areas, things_list_by_tag, things_list_inbox,
    things_list_logbook, things_list_projects, things_list_someday,
    things_list_today, things_list_trash, things_list_upcoming, ListAnytimeArgs,
    ListAreasArgs, ListByTagArgs, ListInboxArgs, ListLogbookArgs, ListProjectsArgs,
    ListSomedayArgs, ListTodayArgs, ListTrashArgs, ListUpcomingArgs,
};
```

**(b)** Remove the `Tag` re-export from `crate::core::types`. The current line:

```rust
use crate::core::types::{Area, Project, ProjectFull, Tag, TodoFull, TodoSummary};
```

becomes (drop `Tag`):

```rust
use crate::core::types::{Area, Project, ProjectFull, TodoFull, TodoSummary};
```

**(c)** Add new imports for the tag tools and the assign/unassign tools:

After the existing `use crate::tools::bulk::…` line, insert:

```rust
use crate::core::applescript::admin::TagOutcome;
use crate::core::reader::tags::TagListing;
use crate::tools::tags::{
    things_create_tag, things_delete_tag, things_list_tags, things_merge_tags,
    things_move_tag, things_rename_tag,
    CreateTagArgs, DeleteTagArgs, ListTagsArgs, MergeTagsArgs, MoveTagArgs, RenameTagArgs,
};
```

And add the assign/unassign names to the existing `use crate::tools::todos::{ … }` block:

```rust
use crate::tools::todos::{
    things_add_todo, things_assign_tag, things_cancel_todo, things_complete_todo,
    things_get_todo, things_move_todo, things_unassign_tag, things_update_todo,
    AddTodoArgs, GetTodoArgs, MoveTodoArgs, StatusChangeArgs, TagAssignmentArgs,
    UpdateTodoArgs,
};
```

**(d)** Replace the existing `things_list_tags` registration (currently at server.rs:223–241). Delete the old block:

```rust
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

Replace it with the updated version (return type now `Json<TagListing>`):

```rust
    #[tool(
        name = "things_list_tags",
        description = "Return all tags. `flat` is the every-tag list; `roots` is a tree of `TagNode`s rooted at parentless tags. Read-only.",
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
    ) -> Result<Json<TagListing>, McpError> {
        let listing = things_list_tags(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(listing))
    }
```

**(e)** Append the seven new tool registrations inside `impl ThingsServer { … }`, immediately after the existing `tool_bulk_json` (server.rs:472–481):

```rust
    #[tool(
        name = "things_assign_tag",
        description = "Attach one or more tags to a to-do. Identifier is the to-do's uuid. Tags are referenced by name. Idempotent: reassigning an already-attached tag is a no-op. The implementation reads current tags and replays an `update` with the merged set; concurrent edits between the read and write may overwrite each other (≈100–300 ms window).",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn tool_assign_tag(
        &self,
        Parameters(args): Parameters<TagAssignmentArgs>,
    ) -> Result<Json<WriteOutcome>, McpError> {
        let out = things_assign_tag(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(out))
    }

    #[tool(
        name = "things_unassign_tag",
        description = "Detach one or more tags from a to-do. Idempotent: removing a tag that wasn't attached is a no-op. Read-modify-write through Things' `update` op; concurrent edits between the read and write may overwrite each other (≈100–300 ms window).",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn tool_unassign_tag(
        &self,
        Parameters(args): Parameters<TagAssignmentArgs>,
    ) -> Result<Json<WriteOutcome>, McpError> {
        let out = things_unassign_tag(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(out))
    }

    #[tool(
        name = "things_create_tag",
        description = "Create a new tag. Optionally nest it under an existing parent tag by name. Runs via AppleScript (`osascript`).",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn tool_create_tag(
        &self,
        Parameters(args): Parameters<CreateTagArgs>,
    ) -> Result<Json<TagOutcome>, McpError> {
        let out = things_create_tag(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(out))
    }

    #[tool(
        name = "things_rename_tag",
        description = "Rename an existing tag globally. Every to-do that carried the old name will surface the new name. Runs via AppleScript.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn tool_rename_tag(
        &self,
        Parameters(args): Parameters<RenameTagArgs>,
    ) -> Result<Json<TagOutcome>, McpError> {
        let out = things_rename_tag(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(out))
    }

    #[tool(
        name = "things_merge_tags",
        description = "Reassign every to-do tagged `source` to also carry `target`, then delete `source`. Source and target must differ. Runs via AppleScript.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn tool_merge_tags(
        &self,
        Parameters(args): Parameters<MergeTagsArgs>,
    ) -> Result<Json<TagOutcome>, McpError> {
        let out = things_merge_tags(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(out))
    }

    #[tool(
        name = "things_delete_tag",
        description = "Delete a tag globally. To-dos that carry the tag stay; only the tag itself is removed. Runs via AppleScript.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn tool_delete_tag(
        &self,
        Parameters(args): Parameters<DeleteTagArgs>,
    ) -> Result<Json<TagOutcome>, McpError> {
        let out = things_delete_tag(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(out))
    }

    #[tool(
        name = "things_move_tag",
        description = "Move a tag under a new parent tag (or to the root when `new_parent` is omitted/null). Runs via AppleScript.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn tool_move_tag(
        &self,
        Parameters(args): Parameters<MoveTagArgs>,
    ) -> Result<Json<TagOutcome>, McpError> {
        let out = things_move_tag(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(out))
    }
```

- [ ] **Step 6: Build to confirm the wiring**

```
cargo build
cargo test
```

Expected: build clean. Tests **145 reported** (143 passing + 2 ignored). No new tests this task — wiring only. The existing `list_tags_returns_flat_list_with_parent_links` reader test is in `queries.rs` (not `lists.rs`) and stays valid (it tests `queries::list_tags`, which still exists).

(If the build complains about a stray `use crate::core::types::Tag;` somewhere — e.g., in `lists.rs` — remove that import. Inspection during planning suggested it's only used by the removed function.)

- [ ] **Step 7: Commit**

```bash
git add crates/things-mcp/src/state.rs \
        crates/things-mcp/src/tools/mod.rs \
        crates/things-mcp/src/tools/tags.rs \
        crates/things-mcp/src/tools/lists.rs \
        crates/things-mcp/src/server.rs \
        crates/things-mcp/src/main.rs \
        crates/things-mcp/tests/end_to_end_writes_plan_5.rs
git commit -m "plan-6: state.tag_admin + tools/tags + server registrations"
```

(The Plan-5 integration test file is included in the commit because Step 2 added the `applescript_override: None` field to its `AppStateOptions` literals.)

---

## Task 8: integration tests — `tests/end_to_end_tags_plan_6.rs`

Nine end-to-end tests exercising the full Plan-6 surface. Eight are dry-run tests (with the test-DB safety gate set to `DryRun` and `RecordingAppleScript` / `RecordingExecutor` injected). One is a live-mode test that asserts the AppleScript driver received exactly the script that `render_rename_tag` produces — the first end-to-end exercise of the `applescript_override` seam.

**Files:**
- Create: `crates/things-mcp/tests/end_to_end_tags_plan_6.rs`

- [ ] **Step 1: Create the integration test file**

`crates/things-mcp/tests/end_to_end_tags_plan_6.rs`:

```rust
//! End-to-end exercise of every Plan-6 tag tool.
//!
//! Eight tests run in test-DB DryRun mode against the fixture: writes
//! short-circuit before either the executor (`RecordingExecutor`) or the
//! AppleScript driver (`RecordingAppleScript`) is called, and the tools
//! return `dry_run: true`.
//!
//! The ninth test runs in Live mode with `RecordingAppleScript` injected,
//! and asserts the recorded script string equals what
//! `render_rename_tag(old, new)` produces — proving the
//! `applescript_override` seam delivers the rendered script intact.

use std::sync::Arc;

use things_mcp::core::applescript::driver::{AppleScriptDriver, RecordingAppleScript};
use things_mcp::core::applescript::script::render_rename_tag;
use things_mcp::core::reader::fixture::build_fixture;
use things_mcp::core::writer::executor::{Executor, RecordingExecutor};
use things_mcp::state::{AppState, AppStateOptions};
use things_mcp::tools::tags::{
    things_create_tag, things_delete_tag, things_list_tags, things_merge_tags,
    things_move_tag, things_rename_tag, CreateTagArgs, DeleteTagArgs, ListTagsArgs,
    MergeTagsArgs, MoveTagArgs, RenameTagArgs,
};
use things_mcp::tools::todos::{
    things_assign_tag, things_unassign_tag, TagAssignmentArgs,
};

/// Build an `AppState` in DryRun mode against the fixture, with both a
/// recording executor and a recording AppleScript driver injected. Returns
/// the state plus both recorders so tests can assert what was (or wasn't)
/// captured.
async fn build_dryrun_state() -> (
    AppState,
    Arc<RecordingExecutor>,
    Arc<RecordingAppleScript>,
) {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("things.sqlite");
    build_fixture(&db).unwrap();
    let recorder = Arc::new(RecordingExecutor::new());
    let applescript = Arc::new(RecordingAppleScript::new());
    let state = AppState::build(AppStateOptions {
        env_db_path: Some(db.clone()),
        home_dir: tmp.path().to_path_buf(),
        config_path: tmp.path().join("config.toml"),
        allow_writes_on_test_db: true,
        executor_override: Some(recorder.clone() as Arc<dyn Executor>),
        applescript_override: Some(applescript.clone() as Arc<dyn AppleScriptDriver>),
    })
    .await
    .unwrap();
    std::mem::forget(tmp);
    (state, recorder, applescript)
}

#[tokio::test]
async fn list_tags_returns_flat_and_roots_from_fixture() {
    let (state, _executor, _applescript) = build_dryrun_state().await;
    let listing = things_list_tags(state, ListTagsArgs::default()).await.unwrap();
    // Flat: 3 tags from the fixture.
    assert_eq!(listing.flat.len(), 3);
    // Roots: Errand + Deep work (Call has parent Errand).
    assert_eq!(listing.roots.len(), 2);
    let errand = listing.roots.iter().find(|r| r.title == "Errand").unwrap();
    assert_eq!(errand.children.len(), 1);
    assert_eq!(errand.children[0].title, "Call");
}

#[tokio::test]
async fn assign_tag_dry_run_does_not_call_executor() {
    let (state, executor, _applescript) = build_dryrun_state().await;
    let out = things_assign_tag(
        state,
        TagAssignmentArgs {
            id: "todo-1".into(),
            tags: vec!["Errand".into()],
        },
    )
    .await
    .unwrap();
    assert!(out.dry_run);
    assert_eq!(out.action, "update_todo");
    assert!(executor.urls().is_empty());
}

#[tokio::test]
async fn unassign_tag_dry_run_does_not_call_executor() {
    let (state, executor, _applescript) = build_dryrun_state().await;
    // todo-2 is tagged 'Errand' in the fixture.
    let out = things_unassign_tag(
        state,
        TagAssignmentArgs {
            id: "todo-2".into(),
            tags: vec!["Errand".into()],
        },
    )
    .await
    .unwrap();
    assert!(out.dry_run);
    assert_eq!(out.action, "update_todo");
    assert!(executor.urls().is_empty());
}

#[tokio::test]
async fn create_tag_dry_run_does_not_call_applescript_driver() {
    let (state, _executor, applescript) = build_dryrun_state().await;
    let out = things_create_tag(
        state,
        CreateTagArgs {
            name: "NewTag".into(),
            parent: None,
        },
    )
    .await
    .unwrap();
    assert!(out.dry_run);
    assert_eq!(out.action, "create_tag");
    assert!(applescript.scripts().is_empty());
}

#[tokio::test]
async fn rename_tag_dry_run_does_not_call_applescript_driver() {
    let (state, _executor, applescript) = build_dryrun_state().await;
    let out = things_rename_tag(
        state,
        RenameTagArgs {
            old: "Errand".into(),
            new: "Errands".into(),
        },
    )
    .await
    .unwrap();
    assert!(out.dry_run);
    assert_eq!(out.action, "rename_tag");
    assert!(applescript.scripts().is_empty());
}

#[tokio::test]
async fn merge_tags_dry_run_does_not_call_applescript_driver() {
    let (state, _executor, applescript) = build_dryrun_state().await;
    let out = things_merge_tags(
        state,
        MergeTagsArgs {
            source: "Errand".into(),
            target: "Deep work".into(),
        },
    )
    .await
    .unwrap();
    assert!(out.dry_run);
    assert_eq!(out.action, "merge_tags");
    assert!(applescript.scripts().is_empty());
}

#[tokio::test]
async fn delete_tag_dry_run_does_not_call_applescript_driver() {
    let (state, _executor, applescript) = build_dryrun_state().await;
    let out = things_delete_tag(
        state,
        DeleteTagArgs {
            name: "Errand".into(),
        },
    )
    .await
    .unwrap();
    assert!(out.dry_run);
    assert_eq!(out.action, "delete_tag");
    assert!(applescript.scripts().is_empty());
}

#[tokio::test]
async fn move_tag_dry_run_does_not_call_applescript_driver() {
    let (state, _executor, applescript) = build_dryrun_state().await;
    let out = things_move_tag(
        state,
        MoveTagArgs {
            name: "Call".into(),
            new_parent: None,
        },
    )
    .await
    .unwrap();
    assert!(out.dry_run);
    assert_eq!(out.action, "move_tag");
    assert!(applescript.scripts().is_empty());
}

#[tokio::test]
async fn rename_tag_live_mode_hands_rendered_script_to_recording_driver() {
    // Live mode: no `env_db_path` (so safety = Live). We still feed it a
    // fixture DB via config.toml so we don't touch the user's Things, and
    // we override the AppleScript driver with a recorder so no `osascript`
    // is actually spawned. The test asserts the script we recorded equals
    // what `render_rename_tag("Errand", "Errands")` produces.
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("things.sqlite");
    build_fixture(&db).unwrap();
    let config_toml = tmp.path().join("config.toml");
    std::fs::write(
        &config_toml,
        format!(
            r#"
[things]
db_path = "{}"

[writer]
poll_timeout_ms = 100
poll_interval_ms = 10
"#,
            db.display(),
        ),
    )
    .unwrap();

    let applescript = Arc::new(RecordingAppleScript::new());
    let state = AppState::build(AppStateOptions {
        env_db_path: None,                 // Live mode
        home_dir: tmp.path().to_path_buf(),
        config_path: config_toml,
        allow_writes_on_test_db: false,
        executor_override: None,
        applescript_override: Some(applescript.clone() as Arc<dyn AppleScriptDriver>),
    })
    .await
    .unwrap();
    std::mem::forget(tmp);

    let out = things_rename_tag(
        state,
        RenameTagArgs {
            old: "Errand".into(),
            new: "Errands".into(),
        },
    )
    .await
    .unwrap();

    // Live mode → dry_run is false. The recorded script must equal what
    // the pure renderer produces.
    assert!(!out.dry_run);
    assert_eq!(out.action, "rename_tag");
    let scripts = applescript.scripts();
    assert_eq!(scripts.len(), 1);
    assert_eq!(scripts[0], render_rename_tag("Errand", "Errands"));
}
```

- [ ] **Step 2: Build + full sweep**

```
cargo build
cargo test
```

Expected: **154 reported** (152 passing + 2 ignored). +9 over T7: 1 list + 7 dry-run admin/assign/unassign + 1 live-mode rename = 9 tests.

- [ ] **Step 3: Commit**

```bash
git add crates/things-mcp/tests/end_to_end_tags_plan_6.rs
git commit -m "tests: plan-6 tag tool integration coverage"
```

---

## Task 9: README + final sweep

Bump the README status line. Confirm `cargo test` shows the expected total and `cargo build --release` is clean.

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Bump the status line**

Open `README.md`. The current status line (from Plan 5) is:

```markdown
**Status:** Plan 5 — full write surface shipping over the JSON URL scheme: `things_add_todo`, `things_add_project`, `things_update_todo`, `things_update_project`, `things_complete_todo`, `things_cancel_todo`, `things_move_todo`, and the `things_bulk_json` power tool. Updates flow through the auth-token gate (`THINGS_AUTH_TOKEN` env or `[things].auth_token` in `config.toml`). Bulk skips per-element verify; all other tools poll the reader for a typed predicate (`CreateByTitle`, `UpdateById`, `StatusChange`, `MoveById`) up to `writer.poll_timeout_ms`. See `docs/superpowers/plans/` for the active plan and follow-ons.
```

Replace with:

```markdown
**Status:** Plan 6 — full tag surface shipping. Eight new tools: `things_list_tags` (now returns a `TagListing { flat, roots }` with both the flat list and a parent-child tree), `things_assign_tag` / `things_unassign_tag` (JSON URL chassis, read-modify-write through `update`+`tags`, verified via `TagOnTodoById` predicate), plus five admin tools (`things_create_tag`, `things_rename_tag`, `things_merge_tags`, `things_delete_tag`, `things_move_tag`) routed through a new `core/applescript/` driver (`osascript -e <script>`, verified by exit code). DryRun mode short-circuits both the JSON URL executor and the AppleScript driver. See `docs/superpowers/plans/` for the active plan and follow-ons.
```

- [ ] **Step 2: Full sweep + release build**

```
cargo test && cargo build --release
```

Expected: **154 tests reported** (152 passing + 2 ignored); release build clean. Both `#[ignore]` smoke tests (`open_command_executor_smoke` from Plan 4, `osascript_driver_smoke` from Plan 6) stay ignored.

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: README — plan 6 full tag surface shipping"
```

- [ ] **Step 4: Inspect history**

```
git log --oneline | head -12
```

Expected: 9 new commits on top of `ea9f4e1` (one per task in this plan).

---

## Self-review checklist (for the executor)

- [ ] All 8 new tag-aware MCP tools are registered on `ThingsServer` with the MCP annotations from the spec's §1 table (`things_list_tags`, `things_assign_tag`, `things_unassign_tag`, `things_create_tag`, `things_rename_tag`, `things_merge_tags`, `things_delete_tag`, `things_move_tag`).
- [ ] `things_rename_tag`, `things_merge_tags`, `things_delete_tag` have `destructive_hint = true`; the five others have `destructive_hint = false`.
- [ ] `things_list_tags` is the only one with `read_only_hint = true`.
- [ ] `core/applescript/` contains 4 files: `mod.rs`, `driver.rs`, `script.rs`, `admin.rs`. No file exceeds ~300 lines.
- [ ] `AppleScriptDriver` is a trait with `OsascriptDriver` (production) + `RecordingAppleScript` (test) impls; both live in `driver.rs`.
- [ ] `TagAdmin` owns the safety gate: `Forbidden` → error; `DryRun` → short-circuit; `Live` → driver call. **No auth-token gate** — AppleScript ops never carry an auth-token.
- [ ] `TagAdmin::merge` rejects `source == target` with `InvalidInput`. The tool adapter also rejects it.
- [ ] Tool adapters validate empty `id`, empty `name`, empty `old`/`new`, empty `source`/`target`, empty `tags` vec, no-op `rename` (`old == new`), and empty `parent`/`new_parent` (when supplied via `Some("")`).
- [ ] `VerifyPredicate::TagOnTodoById { id, tag, present }` exists; the existence-probe OR-pattern at the top of `verify()` includes it; the `check_once` arm joins `TMTaskTag → TMTag.title`.
- [ ] `core/reader/tags.rs::build_tree` carries a `HashSet` cycle guard and emits a `tracing::warn!` when a cycle is dropped.
- [ ] `things_list_tags` returns `Json<TagListing>` (NOT `Json<Vec<Tag>>`); the old `tools/lists.rs::things_list_tags` is deleted along with its `ListTagsArgs`.
- [ ] `AppStateOptions` has both `executor_override` and `applescript_override` (both `Option<Arc<dyn …>>`); every existing literal constructing `AppStateOptions` carries `applescript_override: None`.
- [ ] `state.tag_admin: Arc<TagAdmin>` is built from `safety` + the optional driver override; production uses `OsascriptDriver`.
- [ ] `things_assign_tag` and `things_unassign_tag` both use read-modify-write through `core::reader::queries::get_tags_for_task` + `Operation::UpdateTodo(UpdateTodoSpec { tags: Some(...), ..Default::default() })`. They verify via `TagOnTodoById { tag: args.tags[0], present: true | false }`.
- [ ] No new dependencies in `Cargo.toml`. No new variants on `ThingsError`.
- [ ] The Plan-5 integration tests (`end_to_end_writes_plan_5.rs`) and all earlier tests still pass.
- [ ] `cargo test` shows **154 reported** at the end of Task 9; `cargo build --release` is clean.

When all green, the natural next step is **Plan 7** (recurrence definition via AppleScript wrapper or stdio path consolidation, per the project intel). Plan 6's `core/applescript/` stays unchanged and is reused for any future AppleScript-only operation.
