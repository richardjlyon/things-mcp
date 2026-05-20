# things-mcp Plan 5 — remaining write tools

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Layer the remaining 7 MCP write tools (`add_project`, `update_todo`, `update_project`, `complete_todo`, `cancel_todo`, `move_todo`, `bulk_json`) on top of the Plan 4 chassis. Trip the auth-token gate for the first time end-to-end.

**Architecture:** Plan 4's `core/writer/` chassis stays. Three surgical changes:
1. Split `operation.rs` into a `core/writer/operation/` directory (one file per variant — Plan 4's monolith doesn't scale to 8 variants).
2. Extend `VerifyPredicate::CreateByTitle` with a `kind: TaskKind` field (projects need `type=1`, not `type=0`).
3. Change `Writer::fire`'s second arg from `VerifyPredicate` to `Option<VerifyPredicate>` so `things_bulk_json` can skip verification entirely.

**Tech Stack:** Same as Plans 1–4. No new dependencies. No new `ThingsError` variants.

**Spec:** `docs/superpowers/specs/2026-05-20-plan-5-write-tools-design.md`.

**Predecessor:** `docs/superpowers/plans/2026-05-20-plan-4-writer-infra.md` (HEAD `78ab8b9`, 87 tests). The Cargo.lock recording (`4c619ce`) and this plan's spec (`cd5a0bb`) are also on `main`.

**Scope notes:**
- **All 7 tools at once.** Plan 4's "one tool at a time" mode was for chassis validation. Plan 5 is fan-out; landing the seven together keeps the test surface coherent.
- **First end-to-end auth-token exercise.** `things_update_todo`'s integration test sets `cfg.things.auth_token` and asserts the recorded URL contains `&auth-token=test-token-123`.
- **No live Things calls.** Every integration test uses `RecordingExecutor` + test-DB dry-run mode, just like Plan 4.
- **`things_bulk_json` is a power tool.** It accepts a raw `Vec<serde_json::Value>` and pipes it through `build_url` unchanged. No payload validation beyond `len() > 0 && len() <= 250` (Things' documented rate limit).

**Expected test counts (cumulative):**
| After task | Lib | Integration | Total | Delta |
|---|---|---|---|---|
| Baseline (HEAD `cd5a0bb`) | 82 | 5 | 87 | — |
| T1 (operation/ split) | 82 | 5 | 87 | 0 |
| T2 (AddProject) | 84 | 5 | 89 | +2 |
| T3 (UpdateTodo) | 86 | 5 | 91 | +2 |
| T4 (UpdateProject) | 88 | 5 | 93 | +2 |
| T5 (Complete/Cancel) | 92 | 5 | 97 | +4 |
| T6 (MoveTodo + MoveById) | 96 | 5 | 101 | +4 |
| T7 (BulkRaw + fire(None)) | 99 | 5 | 104 | +3 |
| T8 (7 tool adapters + 8 integration tests) | 99 | 13 | 112 | +8 |
| T9 (README + sweep) | 99 | 13 | 112 | 0 |

---

## File map

**Create (8 new files):**
- `crates/things-mcp/src/core/writer/operation/mod.rs` — enum + dispatch (replaces current `operation.rs`)
- `crates/things-mcp/src/core/writer/operation/add_todo.rs` — `AddTodoSpec` + `render_add_todo` (moved from current `operation.rs`)
- `crates/things-mcp/src/core/writer/operation/add_project.rs`
- `crates/things-mcp/src/core/writer/operation/update_todo.rs`
- `crates/things-mcp/src/core/writer/operation/update_project.rs`
- `crates/things-mcp/src/core/writer/operation/status_change.rs` — `CompleteTodo` + `CancelTodo`
- `crates/things-mcp/src/core/writer/operation/move_todo.rs`
- `crates/things-mcp/src/core/writer/operation/bulk.rs`
- `crates/things-mcp/src/tools/bulk.rs`
- `crates/things-mcp/tests/end_to_end_writes_plan_5.rs` (integration test for all 7 new tools)

**Delete:**
- `crates/things-mcp/src/core/writer/operation.rs` (replaced by the directory)

**Modify:**
- `crates/things-mcp/src/core/writer/mod.rs` (no-op once the path is a directory)
- `crates/things-mcp/src/core/writer/verify.rs` — `CreateByTitle` gains `kind` field; add `MoveById` variant + check
- `crates/things-mcp/src/core/writer/writer.rs` — `fire` signature `Option<VerifyPredicate>` + skip-verify branch
- `crates/things-mcp/src/tools/todos.rs` — add `things_update_todo`, `things_complete_todo`, `things_cancel_todo`, `things_move_todo`; bump `things_add_todo` call sites for signature changes
- `crates/things-mcp/src/tools/projects.rs` — add `things_add_project`, `things_update_project`
- `crates/things-mcp/src/server.rs` — register 7 new tools
- `crates/things-mcp/src/lib.rs` — declare `pub mod bulk` under `tools`
- `crates/things-mcp/tests/end_to_end_add_todo.rs` — no source changes, but the test will be re-run as a regression gate
- `README.md` — status line bump to Plan 5

---

### Task 1: split `operation.rs` into `operation/` directory

Pure mechanical refactor. Move the existing `AddTodoSpec` + `render_add_todo` + their tests into `operation/add_todo.rs`. Make `operation/mod.rs` the new module root. Delete the old `operation.rs`. All 87 existing tests stay green; this task adds none.

**Files:**
- Delete: `crates/things-mcp/src/core/writer/operation.rs`
- Create: `crates/things-mcp/src/core/writer/operation/mod.rs`
- Create: `crates/things-mcp/src/core/writer/operation/add_todo.rs`

- [ ] **Step 1: Create the new `operation/add_todo.rs`**

Create `crates/things-mcp/src/core/writer/operation/add_todo.rs` with the existing `AddTodoSpec` struct, `render_add_todo` function, and tests — but re-rooted under the new module path:

```rust
//! `AddTodoSpec` and its JSON render. One variant of `Operation`.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AddTodoSpec {
    pub title: String,
    pub notes: Option<String>,
    /// `"today"`, `"tomorrow"`, `"evening"`, `"anytime"`, `"someday"`,
    /// or an ISO date / timestamp.
    pub when: Option<String>,
    /// ISO `YYYY-MM-DD`.
    pub deadline: Option<String>,
    pub tags: Vec<String>,
    pub checklist_items: Vec<String>,
    /// Project or area UUID this to-do belongs to.
    pub list_id: Option<String>,
    /// Heading UUID, if the to-do should be filed under a specific heading
    /// inside a project.
    pub heading_id: Option<String>,
}

pub(crate) fn render_add_todo(spec: &AddTodoSpec) -> Value {
    let mut attributes = serde_json::Map::new();
    attributes.insert("title".into(), Value::String(spec.title.clone()));
    if let Some(notes) = spec.notes.as_ref() {
        attributes.insert("notes".into(), Value::String(notes.clone()));
    }
    if let Some(when) = spec.when.as_ref() {
        attributes.insert("when".into(), Value::String(when.clone()));
    }
    if let Some(deadline) = spec.deadline.as_ref() {
        attributes.insert("deadline".into(), Value::String(deadline.clone()));
    }
    if !spec.tags.is_empty() {
        attributes.insert(
            "tags".into(),
            Value::Array(spec.tags.iter().map(|t| Value::String(t.clone())).collect()),
        );
    }
    if !spec.checklist_items.is_empty() {
        attributes.insert(
            "checklist-items".into(),
            Value::Array(
                spec.checklist_items
                    .iter()
                    .map(|t| {
                        json!({
                            "type": "checklist-item",
                            "attributes": { "title": t }
                        })
                    })
                    .collect(),
            ),
        );
    }
    if let Some(id) = spec.list_id.as_ref() {
        attributes.insert("list-id".into(), Value::String(id.clone()));
    }
    if let Some(id) = spec.heading_id.as_ref() {
        attributes.insert("heading".into(), Value::String(id.clone()));
    }

    json!({
        "type": "to-do",
        "attributes": Value::Object(attributes),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::writer::operation::Operation;

    #[test]
    fn add_todo_minimal_renders_title_only() {
        let op = Operation::AddTodo(AddTodoSpec {
            title: "Buy milk".into(),
            ..Default::default()
        });
        let v = op.render_json();
        assert_eq!(v["type"], "to-do");
        assert_eq!(v["attributes"]["title"], "Buy milk");
        let attrs = v["attributes"].as_object().unwrap();
        assert_eq!(attrs.len(), 1);
        assert!(!attrs.contains_key("notes"));
        assert!(!attrs.contains_key("tags"));
        assert!(!attrs.contains_key("checklist-items"));
    }

    #[test]
    fn add_todo_full_renders_every_field() {
        let op = Operation::AddTodo(AddTodoSpec {
            title: "Plan release".into(),
            notes: Some("Coordinate with QA".into()),
            when: Some("today".into()),
            deadline: Some("2026-06-01".into()),
            tags: vec!["Work".into(), "Urgent".into()],
            checklist_items: vec!["Draft notes".into(), "Cut RC".into()],
            list_id: Some("proj-42".into()),
            heading_id: Some("head-7".into()),
        });
        let v = op.render_json();
        let attrs = v["attributes"].as_object().unwrap();
        assert_eq!(attrs["title"], "Plan release");
        assert_eq!(attrs["notes"], "Coordinate with QA");
        assert_eq!(attrs["when"], "today");
        assert_eq!(attrs["deadline"], "2026-06-01");
        assert_eq!(attrs["tags"], serde_json::json!(["Work", "Urgent"]));
        assert_eq!(attrs["list-id"], "proj-42");
        assert_eq!(attrs["heading"], "head-7");
        let checklist = attrs["checklist-items"].as_array().unwrap();
        assert_eq!(checklist.len(), 2);
        assert_eq!(checklist[0]["type"], "checklist-item");
        assert_eq!(checklist[0]["attributes"]["title"], "Draft notes");
    }

    #[test]
    fn action_name_and_auth_requirement() {
        let op = Operation::AddTodo(AddTodoSpec {
            title: "x".into(),
            ..Default::default()
        });
        assert_eq!(op.action_name(), "add_todo");
        assert!(!op.requires_auth_token(), "creates do not require auth-token");
    }
}
```

- [ ] **Step 2: Create the new `operation/mod.rs`**

Create `crates/things-mcp/src/core/writer/operation/mod.rs`:

```rust
//! `Operation` — typed write operations, each capable of rendering itself
//! as a single Things JSON URL operation element.

pub mod add_todo;

pub use add_todo::AddTodoSpec;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Operation {
    AddTodo(AddTodoSpec),
}

impl Operation {
    /// Stable snake-case action name surfaced in `WriteOutcome.action`.
    pub fn action_name(&self) -> &'static str {
        match self {
            Operation::AddTodo(_) => "add_todo",
        }
    }

    /// `true` iff this operation type needs Things' auth-token (i.e. it's an
    /// `update`). Creates pass through without one.
    pub fn requires_auth_token(&self) -> bool {
        match self {
            Operation::AddTodo(_) => false,
        }
    }

    /// Render this operation as a single element of the JSON array payload
    /// Things expects in `things:///json?data=…`.
    pub fn render_json(&self) -> Value {
        match self {
            Operation::AddTodo(spec) => add_todo::render_add_todo(spec),
        }
    }
}
```

- [ ] **Step 3: Delete the old `operation.rs`**

```bash
rm crates/things-mcp/src/core/writer/operation.rs
```

- [ ] **Step 4: Verify the build + tests**

```
cargo build
cargo test
```

Expected: build clean. Full suite **87 total** (82 lib + 5 integration; 1 ignored). No new tests; this is a pure refactor.

- [ ] **Step 5: Commit**

```bash
git add crates/things-mcp/src/core/writer/operation.rs crates/things-mcp/src/core/writer/operation
git commit -m "core/writer/operation: split into per-variant module directory"
```

---

### Task 2: `AddProject` + extend `CreateByTitle` with `TaskKind`

Add the `AddProject(AddProjectSpec)` variant and its render. Extend `VerifyPredicate::CreateByTitle` with a `kind: TaskKind` field so projects (type=1) can use the same predicate as to-dos (type=0). All existing CreateByTitle call sites are updated atomically to pass `TaskKind::Todo`.

**Files:**
- Create: `crates/things-mcp/src/core/writer/operation/add_project.rs`
- Modify: `crates/things-mcp/src/core/writer/operation/mod.rs`
- Modify: `crates/things-mcp/src/core/writer/verify.rs`
- Modify: `crates/things-mcp/src/core/writer/writer.rs` (test helper updated)
- Modify: `crates/things-mcp/src/tools/todos.rs` (`things_add_todo` updated)

- [ ] **Step 1: Create `operation/add_project.rs`**

`crates/things-mcp/src/core/writer/operation/add_project.rs`:

```rust
//! `AddProjectSpec` and its JSON render. Creates a Things project, optionally
//! with initial headings and to-dos nested inside via Things' `items` array.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::core::writer::operation::add_todo::AddTodoSpec;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AddProjectSpec {
    pub title: String,
    pub notes: Option<String>,
    pub when: Option<String>,
    pub deadline: Option<String>,
    pub tags: Vec<String>,
    /// Parent area UUID. Optional — projects live in "no area" if omitted.
    pub area_id: Option<String>,
    /// Initial to-dos to nest inside the project. Order preserved.
    pub todos: Vec<AddTodoSpec>,
    /// Initial heading titles. Order preserved. Renders before todos in the
    /// items[] array — a Things UX convention, not a hard rule.
    pub headings: Vec<String>,
}

pub(crate) fn render_add_project(spec: &AddProjectSpec) -> Value {
    let mut attributes = serde_json::Map::new();
    attributes.insert("title".into(), Value::String(spec.title.clone()));
    if let Some(notes) = spec.notes.as_ref() {
        attributes.insert("notes".into(), Value::String(notes.clone()));
    }
    if let Some(when) = spec.when.as_ref() {
        attributes.insert("when".into(), Value::String(when.clone()));
    }
    if let Some(deadline) = spec.deadline.as_ref() {
        attributes.insert("deadline".into(), Value::String(deadline.clone()));
    }
    if !spec.tags.is_empty() {
        attributes.insert(
            "tags".into(),
            Value::Array(spec.tags.iter().map(|t| Value::String(t.clone())).collect()),
        );
    }
    if let Some(id) = spec.area_id.as_ref() {
        attributes.insert("area-id".into(), Value::String(id.clone()));
    }

    // items[] = headings first, then to-dos. Order matches the Things app's
    // typical project layout.
    if !spec.headings.is_empty() || !spec.todos.is_empty() {
        let mut items: Vec<Value> = Vec::with_capacity(spec.headings.len() + spec.todos.len());
        for h in &spec.headings {
            items.push(json!({
                "type": "heading",
                "attributes": { "title": h }
            }));
        }
        for t in &spec.todos {
            // Reuse AddTodoSpec's render via Operation dispatch — but we only
            // need the element shape, not the wrapped Operation. Inline the
            // render here to avoid coupling enum dispatch into the project
            // render.
            items.push(crate::core::writer::operation::add_todo::render_add_todo(t));
        }
        attributes.insert("items".into(), Value::Array(items));
    }

    json!({
        "type": "project",
        "attributes": Value::Object(attributes),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::writer::operation::Operation;

    #[test]
    fn add_project_minimal_renders_title_only() {
        let op = Operation::AddProject(AddProjectSpec {
            title: "Launch website".into(),
            ..Default::default()
        });
        let v = op.render_json();
        assert_eq!(v["type"], "project");
        assert_eq!(v["attributes"]["title"], "Launch website");
        let attrs = v["attributes"].as_object().unwrap();
        assert_eq!(attrs.len(), 1, "only `title` should be set for minimal project");
        assert!(!attrs.contains_key("items"));
        assert!(!attrs.contains_key("area-id"));
    }

    #[test]
    fn add_project_full_with_nested_items() {
        let op = Operation::AddProject(AddProjectSpec {
            title: "Q3 launch".into(),
            notes: Some("Coordinate with marketing".into()),
            when: Some("anytime".into()),
            deadline: Some("2026-09-30".into()),
            tags: vec!["Work".into()],
            area_id: Some("area-2".into()),
            todos: vec![
                AddTodoSpec {
                    title: "Draft press release".into(),
                    ..Default::default()
                },
            ],
            headings: vec!["Design".into(), "QA".into()],
        });
        let v = op.render_json();
        let attrs = v["attributes"].as_object().unwrap();
        assert_eq!(attrs["title"], "Q3 launch");
        assert_eq!(attrs["notes"], "Coordinate with marketing");
        assert_eq!(attrs["area-id"], "area-2");
        assert_eq!(attrs["tags"], serde_json::json!(["Work"]));
        let items = attrs["items"].as_array().unwrap();
        // 2 headings + 1 to-do, headings first.
        assert_eq!(items.len(), 3);
        assert_eq!(items[0]["type"], "heading");
        assert_eq!(items[0]["attributes"]["title"], "Design");
        assert_eq!(items[1]["type"], "heading");
        assert_eq!(items[1]["attributes"]["title"], "QA");
        assert_eq!(items[2]["type"], "to-do");
        assert_eq!(items[2]["attributes"]["title"], "Draft press release");
    }
}
```

- [ ] **Step 2: Wire the variant into `operation/mod.rs`**

Edit `crates/things-mcp/src/core/writer/operation/mod.rs`:

```rust
//! `Operation` — typed write operations, each capable of rendering itself
//! as a single Things JSON URL operation element.

pub mod add_project;
pub mod add_todo;

pub use add_project::AddProjectSpec;
pub use add_todo::AddTodoSpec;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Operation {
    AddTodo(AddTodoSpec),
    AddProject(AddProjectSpec),
}

impl Operation {
    pub fn action_name(&self) -> &'static str {
        match self {
            Operation::AddTodo(_) => "add_todo",
            Operation::AddProject(_) => "add_project",
        }
    }

    pub fn requires_auth_token(&self) -> bool {
        match self {
            Operation::AddTodo(_) => false,
            Operation::AddProject(_) => false,
        }
    }

    pub fn render_json(&self) -> Value {
        match self {
            Operation::AddTodo(spec) => add_todo::render_add_todo(spec),
            Operation::AddProject(spec) => add_project::render_add_project(spec),
        }
    }
}
```

- [ ] **Step 3: Extend `VerifyPredicate::CreateByTitle` with `kind`**

Edit `crates/things-mcp/src/core/writer/verify.rs`. Update imports + the CreateByTitle variant + the SQL:

Replace the `use crate::core::types::{TaskStatus, TodoSummary};` line with:

```rust
use crate::core::types::{TaskKind, TaskStatus, TodoSummary};
```

Replace the `CreateByTitle { title: String, since_unix: f64 }` variant with:

```rust
    /// A row with this title and creationDate ≥ since_unix should exist.
    /// `kind` selects the row's `type` column: Todo → 0, Project → 1.
    CreateByTitle {
        title: String,
        since_unix: f64,
        kind: TaskKind,
    },
```

Update the `check_once` arm for `CreateByTitle` — the `type` filter becomes parameterised:

```rust
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
```

Update the verify tests at the bottom of `verify.rs` — both `verify_create_by_title_finds_existing_row` and `verify_create_by_title_times_out_when_title_absent` need `kind: TaskKind::Todo` added to the CreateByTitle literal:

Replace each `VerifyPredicate::CreateByTitle { title: "...".into(), since_unix: 0.0 }` with `VerifyPredicate::CreateByTitle { title: "...".into(), since_unix: 0.0, kind: TaskKind::Todo }`. There are two such literals to update.

- [ ] **Step 4: Update `writer.rs`'s `pred()` test helper**

Edit `crates/things-mcp/src/core/writer/writer.rs`. The test module has a helper:

```rust
    fn pred(title: &str) -> VerifyPredicate {
        VerifyPredicate::CreateByTitle {
            title: title.into(),
            since_unix: 0.0,
        }
    }
```

Replace with:

```rust
    fn pred(title: &str) -> VerifyPredicate {
        use crate::core::types::TaskKind;
        VerifyPredicate::CreateByTitle {
            title: title.into(),
            since_unix: 0.0,
            kind: TaskKind::Todo,
        }
    }
```

- [ ] **Step 5: Update `things_add_todo` in `tools/todos.rs`**

Find the predicate construction inside `things_add_todo`:

```rust
    let predicate = VerifyPredicate::CreateByTitle {
        title: args.title,
        since_unix,
    };
```

Replace with:

```rust
    let predicate = VerifyPredicate::CreateByTitle {
        title: args.title,
        since_unix,
        kind: crate::core::types::TaskKind::Todo,
    };
```

- [ ] **Step 6: Build + full sweep**

```
cargo build
cargo test
```

Expected: **89 total** (84 lib + 5 integration). +2 over T1: the two new `add_project` render tests.

- [ ] **Step 7: Commit**

```bash
git add crates/things-mcp/src/core/writer/operation/add_project.rs \
        crates/things-mcp/src/core/writer/operation/mod.rs \
        crates/things-mcp/src/core/writer/verify.rs \
        crates/things-mcp/src/core/writer/writer.rs \
        crates/things-mcp/src/tools/todos.rs
git commit -m "core/writer: AddProject variant + CreateByTitle.kind extension"
```

---

### Task 3: `UpdateTodo` variant + render

Adds the `UpdateTodo(UpdateTodoSpec)` variant. Renders as a Things "update" operation (`"operation": "update"`, top-level `"id"`, only-populated attributes in the `attributes` object). This is the first variant that returns `true` from `requires_auth_token()`.

**Files:**
- Create: `crates/things-mcp/src/core/writer/operation/update_todo.rs`
- Modify: `crates/things-mcp/src/core/writer/operation/mod.rs`

- [ ] **Step 1: Create `operation/update_todo.rs`**

```rust
//! `UpdateTodoSpec` and its JSON render. Renders a Things "update" operation
//! (`"operation": "update"`) with the top-level `id` and only populated
//! attributes in `attributes{}`.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateTodoSpec {
    /// UUID of the to-do to update.
    pub id: String,
    /// `None` = leave field unchanged. `Some(value)` = set field to value.
    pub title: Option<String>,
    pub notes: Option<String>,
    pub when: Option<String>,
    pub deadline: Option<String>,
    /// `None` = leave tags unchanged. `Some(vec![])` = clear all tags.
    /// `Some(non_empty)` = replace tags with the given set.
    pub tags: Option<Vec<String>>,
    /// Project or area UUID. `None` = leave alone. `Some("inbox")` = move
    /// to Inbox. `Some(uuid)` = move under the given list.
    pub list_id: Option<String>,
    /// Set `true` to mark the to-do completed; `false` to un-complete.
    pub completed: Option<bool>,
    /// Set `true` to mark canceled; `false` to un-cancel.
    pub canceled: Option<bool>,
}

pub(crate) fn render_update_todo(spec: &UpdateTodoSpec) -> Value {
    let mut attributes = serde_json::Map::new();
    if let Some(v) = spec.title.as_ref() {
        attributes.insert("title".into(), Value::String(v.clone()));
    }
    if let Some(v) = spec.notes.as_ref() {
        attributes.insert("notes".into(), Value::String(v.clone()));
    }
    if let Some(v) = spec.when.as_ref() {
        attributes.insert("when".into(), Value::String(v.clone()));
    }
    if let Some(v) = spec.deadline.as_ref() {
        attributes.insert("deadline".into(), Value::String(v.clone()));
    }
    if let Some(tags) = spec.tags.as_ref() {
        attributes.insert(
            "tags".into(),
            Value::Array(tags.iter().map(|t| Value::String(t.clone())).collect()),
        );
    }
    if let Some(v) = spec.list_id.as_ref() {
        attributes.insert("list-id".into(), Value::String(v.clone()));
    }
    if let Some(v) = spec.completed {
        attributes.insert("completed".into(), Value::Bool(v));
    }
    if let Some(v) = spec.canceled {
        attributes.insert("canceled".into(), Value::Bool(v));
    }

    json!({
        "type": "to-do",
        "operation": "update",
        "id": spec.id,
        "attributes": Value::Object(attributes),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::writer::operation::Operation;

    #[test]
    fn update_todo_minimal_only_id_no_attributes() {
        let op = Operation::UpdateTodo(UpdateTodoSpec {
            id: "todo-1".into(),
            ..Default::default()
        });
        let v = op.render_json();
        assert_eq!(v["type"], "to-do");
        assert_eq!(v["operation"], "update");
        assert_eq!(v["id"], "todo-1");
        // Empty attributes object — Things treats this as a no-op update.
        assert_eq!(v["attributes"].as_object().unwrap().len(), 0);
    }

    #[test]
    fn update_todo_full_renders_all_populated_fields() {
        let op = Operation::UpdateTodo(UpdateTodoSpec {
            id: "todo-1".into(),
            title: Some("New title".into()),
            notes: Some("New notes".into()),
            when: Some("today".into()),
            deadline: Some("2026-12-31".into()),
            tags: Some(vec!["Tag A".into(), "Tag B".into()]),
            list_id: Some("proj-1".into()),
            completed: Some(true),
            canceled: Some(false),
        });
        let v = op.render_json();
        assert_eq!(v["id"], "todo-1");
        let attrs = v["attributes"].as_object().unwrap();
        assert_eq!(attrs["title"], "New title");
        assert_eq!(attrs["notes"], "New notes");
        assert_eq!(attrs["when"], "today");
        assert_eq!(attrs["deadline"], "2026-12-31");
        assert_eq!(attrs["tags"], serde_json::json!(["Tag A", "Tag B"]));
        assert_eq!(attrs["list-id"], "proj-1");
        assert_eq!(attrs["completed"], true);
        assert_eq!(attrs["canceled"], false);
    }
}
```

- [ ] **Step 2: Wire into `operation/mod.rs`**

Edit `crates/things-mcp/src/core/writer/operation/mod.rs`:

Add `pub mod update_todo;` and `pub use update_todo::UpdateTodoSpec;`. Then add the variant + dispatch arms:

```rust
pub mod add_project;
pub mod add_todo;
pub mod update_todo;

pub use add_project::AddProjectSpec;
pub use add_todo::AddTodoSpec;
pub use update_todo::UpdateTodoSpec;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Operation {
    AddTodo(AddTodoSpec),
    AddProject(AddProjectSpec),
    UpdateTodo(UpdateTodoSpec),
}

impl Operation {
    pub fn action_name(&self) -> &'static str {
        match self {
            Operation::AddTodo(_) => "add_todo",
            Operation::AddProject(_) => "add_project",
            Operation::UpdateTodo(_) => "update_todo",
        }
    }

    pub fn requires_auth_token(&self) -> bool {
        match self {
            Operation::AddTodo(_) => false,
            Operation::AddProject(_) => false,
            Operation::UpdateTodo(_) => true,
        }
    }

    pub fn render_json(&self) -> Value {
        match self {
            Operation::AddTodo(spec) => add_todo::render_add_todo(spec),
            Operation::AddProject(spec) => add_project::render_add_project(spec),
            Operation::UpdateTodo(spec) => update_todo::render_update_todo(spec),
        }
    }
}
```

- [ ] **Step 3: Build + full sweep**

```
cargo build
cargo test
```

Expected: **91 total** (86 lib + 5 integration). +2 over T2: the two new `update_todo` render tests.

- [ ] **Step 4: Commit**

```bash
git add crates/things-mcp/src/core/writer/operation/update_todo.rs \
        crates/things-mcp/src/core/writer/operation/mod.rs
git commit -m "core/writer/operation: UpdateTodo variant"
```

---

### Task 4: `UpdateProject` variant + render

Symmetric to `UpdateTodo` but for projects. Renders `"type": "project"` + `"operation": "update"`.

**Files:**
- Create: `crates/things-mcp/src/core/writer/operation/update_project.rs`
- Modify: `crates/things-mcp/src/core/writer/operation/mod.rs`

- [ ] **Step 1: Create `operation/update_project.rs`**

```rust
//! `UpdateProjectSpec` and its JSON render. Renders a Things project
//! "update" operation.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateProjectSpec {
    pub id: String,
    pub title: Option<String>,
    pub notes: Option<String>,
    pub when: Option<String>,
    pub deadline: Option<String>,
    pub tags: Option<Vec<String>>,
    /// Parent area UUID. `Some("inbox")` not meaningful for projects — pass an
    /// area UUID or omit.
    pub area_id: Option<String>,
    pub completed: Option<bool>,
    pub canceled: Option<bool>,
}

pub(crate) fn render_update_project(spec: &UpdateProjectSpec) -> Value {
    let mut attributes = serde_json::Map::new();
    if let Some(v) = spec.title.as_ref() {
        attributes.insert("title".into(), Value::String(v.clone()));
    }
    if let Some(v) = spec.notes.as_ref() {
        attributes.insert("notes".into(), Value::String(v.clone()));
    }
    if let Some(v) = spec.when.as_ref() {
        attributes.insert("when".into(), Value::String(v.clone()));
    }
    if let Some(v) = spec.deadline.as_ref() {
        attributes.insert("deadline".into(), Value::String(v.clone()));
    }
    if let Some(tags) = spec.tags.as_ref() {
        attributes.insert(
            "tags".into(),
            Value::Array(tags.iter().map(|t| Value::String(t.clone())).collect()),
        );
    }
    if let Some(v) = spec.area_id.as_ref() {
        attributes.insert("area-id".into(), Value::String(v.clone()));
    }
    if let Some(v) = spec.completed {
        attributes.insert("completed".into(), Value::Bool(v));
    }
    if let Some(v) = spec.canceled {
        attributes.insert("canceled".into(), Value::Bool(v));
    }

    json!({
        "type": "project",
        "operation": "update",
        "id": spec.id,
        "attributes": Value::Object(attributes),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::writer::operation::Operation;

    #[test]
    fn update_project_minimal_only_id() {
        let op = Operation::UpdateProject(UpdateProjectSpec {
            id: "proj-1".into(),
            ..Default::default()
        });
        let v = op.render_json();
        assert_eq!(v["type"], "project");
        assert_eq!(v["operation"], "update");
        assert_eq!(v["id"], "proj-1");
        assert_eq!(v["attributes"].as_object().unwrap().len(), 0);
    }

    #[test]
    fn update_project_full_renders_all_populated_fields() {
        let op = Operation::UpdateProject(UpdateProjectSpec {
            id: "proj-1".into(),
            title: Some("Renamed".into()),
            notes: Some("Updated notes".into()),
            when: Some("today".into()),
            deadline: Some("2026-12-31".into()),
            tags: Some(vec!["Work".into()]),
            area_id: Some("area-2".into()),
            completed: Some(true),
            canceled: None,
        });
        let v = op.render_json();
        assert_eq!(v["id"], "proj-1");
        let attrs = v["attributes"].as_object().unwrap();
        assert_eq!(attrs["title"], "Renamed");
        assert_eq!(attrs["notes"], "Updated notes");
        assert_eq!(attrs["when"], "today");
        assert_eq!(attrs["deadline"], "2026-12-31");
        assert_eq!(attrs["tags"], serde_json::json!(["Work"]));
        assert_eq!(attrs["area-id"], "area-2");
        assert_eq!(attrs["completed"], true);
        assert!(!attrs.contains_key("canceled"), "None should not render");
    }
}
```

- [ ] **Step 2: Wire into `operation/mod.rs`**

Edit `crates/things-mcp/src/core/writer/operation/mod.rs`:

```rust
pub mod add_project;
pub mod add_todo;
pub mod update_project;
pub mod update_todo;

pub use add_project::AddProjectSpec;
pub use add_todo::AddTodoSpec;
pub use update_project::UpdateProjectSpec;
pub use update_todo::UpdateTodoSpec;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Operation {
    AddTodo(AddTodoSpec),
    AddProject(AddProjectSpec),
    UpdateTodo(UpdateTodoSpec),
    UpdateProject(UpdateProjectSpec),
}

impl Operation {
    pub fn action_name(&self) -> &'static str {
        match self {
            Operation::AddTodo(_) => "add_todo",
            Operation::AddProject(_) => "add_project",
            Operation::UpdateTodo(_) => "update_todo",
            Operation::UpdateProject(_) => "update_project",
        }
    }

    pub fn requires_auth_token(&self) -> bool {
        match self {
            Operation::AddTodo(_) => false,
            Operation::AddProject(_) => false,
            Operation::UpdateTodo(_) => true,
            Operation::UpdateProject(_) => true,
        }
    }

    pub fn render_json(&self) -> Value {
        match self {
            Operation::AddTodo(spec) => add_todo::render_add_todo(spec),
            Operation::AddProject(spec) => add_project::render_add_project(spec),
            Operation::UpdateTodo(spec) => update_todo::render_update_todo(spec),
            Operation::UpdateProject(spec) => update_project::render_update_project(spec),
        }
    }
}
```

- [ ] **Step 3: Build + full sweep**

```
cargo build
cargo test
```

Expected: **93 total** (88 lib + 5 integration). +2 over T3.

- [ ] **Step 4: Commit**

```bash
git add crates/things-mcp/src/core/writer/operation/update_project.rs \
        crates/things-mcp/src/core/writer/operation/mod.rs
git commit -m "core/writer/operation: UpdateProject variant"
```

---

### Task 5: `CompleteTodo` + `CancelTodo` variants

Two narrow status-change variants. Both render as a trivial update with one boolean attribute. Same file because they're structural duals; both `requires_auth_token() = true`.

**Files:**
- Create: `crates/things-mcp/src/core/writer/operation/status_change.rs`
- Modify: `crates/things-mcp/src/core/writer/operation/mod.rs`

- [ ] **Step 1: Create `operation/status_change.rs`**

```rust
//! `CompleteTodo { id }` + `CancelTodo { id }` — narrow status-change updates
//! that share a rendering shape (a tiny update with one boolean attribute).

use serde_json::{json, Value};

pub(crate) fn render_complete_todo(id: &str) -> Value {
    json!({
        "type": "to-do",
        "operation": "update",
        "id": id,
        "attributes": { "completed": true },
    })
}

pub(crate) fn render_cancel_todo(id: &str) -> Value {
    json!({
        "type": "to-do",
        "operation": "update",
        "id": id,
        "attributes": { "canceled": true },
    })
}

#[cfg(test)]
mod tests {
    use crate::core::writer::operation::Operation;

    #[test]
    fn complete_todo_renders_completed_true() {
        let op = Operation::CompleteTodo { id: "todo-1".into() };
        let v = op.render_json();
        assert_eq!(v["type"], "to-do");
        assert_eq!(v["operation"], "update");
        assert_eq!(v["id"], "todo-1");
        assert_eq!(v["attributes"]["completed"], true);
        // No other attributes.
        assert_eq!(v["attributes"].as_object().unwrap().len(), 1);
    }

    #[test]
    fn cancel_todo_renders_canceled_true() {
        let op = Operation::CancelTodo { id: "todo-2".into() };
        let v = op.render_json();
        assert_eq!(v["type"], "to-do");
        assert_eq!(v["operation"], "update");
        assert_eq!(v["id"], "todo-2");
        assert_eq!(v["attributes"]["canceled"], true);
        assert_eq!(v["attributes"].as_object().unwrap().len(), 1);
    }

    #[test]
    fn complete_and_cancel_action_names_and_auth() {
        let c = Operation::CompleteTodo { id: "x".into() };
        let x = Operation::CancelTodo { id: "x".into() };
        assert_eq!(c.action_name(), "complete_todo");
        assert_eq!(x.action_name(), "cancel_todo");
        assert!(c.requires_auth_token());
        assert!(x.requires_auth_token());
    }

    #[test]
    fn complete_distinct_from_cancel() {
        let c = Operation::CompleteTodo { id: "x".into() }.render_json();
        let x = Operation::CancelTodo { id: "x".into() }.render_json();
        // The two MUST emit different boolean keys — they are not interchangeable.
        assert!(c["attributes"].as_object().unwrap().contains_key("completed"));
        assert!(x["attributes"].as_object().unwrap().contains_key("canceled"));
        assert!(!c["attributes"].as_object().unwrap().contains_key("canceled"));
        assert!(!x["attributes"].as_object().unwrap().contains_key("completed"));
    }
}
```

- [ ] **Step 2: Wire into `operation/mod.rs`**

```rust
pub mod add_project;
pub mod add_todo;
pub mod status_change;
pub mod update_project;
pub mod update_todo;

pub use add_project::AddProjectSpec;
pub use add_todo::AddTodoSpec;
pub use update_project::UpdateProjectSpec;
pub use update_todo::UpdateTodoSpec;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Operation {
    AddTodo(AddTodoSpec),
    AddProject(AddProjectSpec),
    UpdateTodo(UpdateTodoSpec),
    UpdateProject(UpdateProjectSpec),
    CompleteTodo { id: String },
    CancelTodo { id: String },
}

impl Operation {
    pub fn action_name(&self) -> &'static str {
        match self {
            Operation::AddTodo(_) => "add_todo",
            Operation::AddProject(_) => "add_project",
            Operation::UpdateTodo(_) => "update_todo",
            Operation::UpdateProject(_) => "update_project",
            Operation::CompleteTodo { .. } => "complete_todo",
            Operation::CancelTodo { .. } => "cancel_todo",
        }
    }

    pub fn requires_auth_token(&self) -> bool {
        match self {
            Operation::AddTodo(_) => false,
            Operation::AddProject(_) => false,
            Operation::UpdateTodo(_) => true,
            Operation::UpdateProject(_) => true,
            Operation::CompleteTodo { .. } => true,
            Operation::CancelTodo { .. } => true,
        }
    }

    pub fn render_json(&self) -> Value {
        match self {
            Operation::AddTodo(spec) => add_todo::render_add_todo(spec),
            Operation::AddProject(spec) => add_project::render_add_project(spec),
            Operation::UpdateTodo(spec) => update_todo::render_update_todo(spec),
            Operation::UpdateProject(spec) => update_project::render_update_project(spec),
            Operation::CompleteTodo { id } => status_change::render_complete_todo(id),
            Operation::CancelTodo { id } => status_change::render_cancel_todo(id),
        }
    }
}
```

- [ ] **Step 3: Build + full sweep**

```
cargo build
cargo test
```

Expected: **97 total** (92 lib + 5 integration). +4 over T4.

- [ ] **Step 4: Commit**

```bash
git add crates/things-mcp/src/core/writer/operation/status_change.rs \
        crates/things-mcp/src/core/writer/operation/mod.rs
git commit -m "core/writer/operation: CompleteTodo + CancelTodo variants"
```

---

### Task 6: `MoveTodo` variant + `VerifyPredicate::MoveById`

Adds the `MoveTodo(MoveTodoSpec)` variant. Its render is a tiny update with `list-id` (or `"inbox"` for the no-parent case). Adds a new `VerifyPredicate::MoveById { id, expected_list_id }` that confirms the post-move row's `project` or `area` column matches the expected target. The check compares each candidate column to the expected value; "inbox" target is encoded as both columns being NULL.

**Files:**
- Create: `crates/things-mcp/src/core/writer/operation/move_todo.rs`
- Modify: `crates/things-mcp/src/core/writer/operation/mod.rs`
- Modify: `crates/things-mcp/src/core/writer/verify.rs`

- [ ] **Step 1: Create `operation/move_todo.rs`**

```rust
//! `MoveTodoSpec` — relocate a to-do under a project, area, or to the Inbox.
//! Renders as a Things update with a `list-id` attribute. `None` maps to the
//! special `"inbox"` value (the Things URL scheme's sentinel for no parent).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MoveTodoSpec {
    pub id: String,
    /// `Some(uuid)` = move to that project or area. `None` = move to Inbox.
    pub list_id: Option<String>,
}

pub(crate) fn render_move_todo(spec: &MoveTodoSpec) -> Value {
    let target = spec.list_id.clone().unwrap_or_else(|| "inbox".to_string());
    json!({
        "type": "to-do",
        "operation": "update",
        "id": spec.id,
        "attributes": { "list-id": target },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::writer::operation::Operation;

    #[test]
    fn move_todo_to_named_list() {
        let op = Operation::MoveTodo(MoveTodoSpec {
            id: "todo-1".into(),
            list_id: Some("proj-1".into()),
        });
        let v = op.render_json();
        assert_eq!(v["type"], "to-do");
        assert_eq!(v["operation"], "update");
        assert_eq!(v["id"], "todo-1");
        assert_eq!(v["attributes"]["list-id"], "proj-1");
    }

    #[test]
    fn move_todo_to_inbox_uses_inbox_sentinel() {
        let op = Operation::MoveTodo(MoveTodoSpec {
            id: "todo-1".into(),
            list_id: None,
        });
        let v = op.render_json();
        assert_eq!(v["attributes"]["list-id"], "inbox");
    }
}
```

- [ ] **Step 2: Wire into `operation/mod.rs`**

```rust
pub mod add_project;
pub mod add_todo;
pub mod move_todo;
pub mod status_change;
pub mod update_project;
pub mod update_todo;

pub use add_project::AddProjectSpec;
pub use add_todo::AddTodoSpec;
pub use move_todo::MoveTodoSpec;
pub use update_project::UpdateProjectSpec;
pub use update_todo::UpdateTodoSpec;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Operation {
    AddTodo(AddTodoSpec),
    AddProject(AddProjectSpec),
    UpdateTodo(UpdateTodoSpec),
    UpdateProject(UpdateProjectSpec),
    CompleteTodo { id: String },
    CancelTodo { id: String },
    MoveTodo(MoveTodoSpec),
}

impl Operation {
    pub fn action_name(&self) -> &'static str {
        match self {
            Operation::AddTodo(_) => "add_todo",
            Operation::AddProject(_) => "add_project",
            Operation::UpdateTodo(_) => "update_todo",
            Operation::UpdateProject(_) => "update_project",
            Operation::CompleteTodo { .. } => "complete_todo",
            Operation::CancelTodo { .. } => "cancel_todo",
            Operation::MoveTodo(_) => "move_todo",
        }
    }

    pub fn requires_auth_token(&self) -> bool {
        match self {
            Operation::AddTodo(_) => false,
            Operation::AddProject(_) => false,
            Operation::UpdateTodo(_) => true,
            Operation::UpdateProject(_) => true,
            Operation::CompleteTodo { .. } => true,
            Operation::CancelTodo { .. } => true,
            Operation::MoveTodo(_) => true,
        }
    }

    pub fn render_json(&self) -> Value {
        match self {
            Operation::AddTodo(spec) => add_todo::render_add_todo(spec),
            Operation::AddProject(spec) => add_project::render_add_project(spec),
            Operation::UpdateTodo(spec) => update_todo::render_update_todo(spec),
            Operation::UpdateProject(spec) => update_project::render_update_project(spec),
            Operation::CompleteTodo { id } => status_change::render_complete_todo(id),
            Operation::CancelTodo { id } => status_change::render_cancel_todo(id),
            Operation::MoveTodo(spec) => move_todo::render_move_todo(spec),
        }
    }
}
```

- [ ] **Step 3: Add `MoveById` to `VerifyPredicate`**

Edit `crates/things-mcp/src/core/writer/verify.rs`.

Add the new variant to the `VerifyPredicate` enum (placed after `StatusChange`):

```rust
    /// The row at this id should have its project/area column set to the
    /// expected_list_id. `Some(uuid)` matches when t.project = uuid OR
    /// t.area = uuid. `None` matches when BOTH columns are NULL (the inbox).
    MoveById {
        id: String,
        expected_list_id: Option<String>,
    },
```

Update the existence-probe match at the top of `verify()` to also cover `MoveById`:

```rust
    if let VerifyPredicate::UpdateById { id, .. }
        | VerifyPredicate::StatusChange { id, .. }
        | VerifyPredicate::MoveById { id, .. } = &pred
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
```

Add the corresponding `check_once` arm at the bottom of the match:

```rust
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
```

Add two new tests inside `verify.rs`'s `#[cfg(test)] mod tests` block:

```rust
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
```

- [ ] **Step 4: Build + full sweep**

```
cargo build
cargo test
```

Expected: **101 total** (96 lib + 5 integration). +4 over T5: 2 move_todo render tests + 2 MoveById verify tests.

- [ ] **Step 5: Commit**

```bash
git add crates/things-mcp/src/core/writer/operation/move_todo.rs \
        crates/things-mcp/src/core/writer/operation/mod.rs \
        crates/things-mcp/src/core/writer/verify.rs
git commit -m "core/writer: MoveTodo variant + MoveById verify predicate"
```

---

### Task 7: `BulkRaw` variant + `Writer::fire` signature change to `Option<VerifyPredicate>`

`BulkRaw` is a passthrough: it carries a `Vec<serde_json::Value>` and renders by emitting them unchanged. Because there's no single "what should the SQLite reader show?" predicate for a bulk write, `Writer::fire` is extended to take `Option<VerifyPredicate>` — when `None`, fire calls the executor and skips the verify step, returning `WriteOutcome { verified: false, id: None }` after a clean executor call.

This task makes two breaking changes that need to be applied atomically: the `BulkRaw` variant introduction, and the `fire` signature change. All existing callers (`things_add_todo` + writer unit tests) get updated in the same commit.

**Files:**
- Create: `crates/things-mcp/src/core/writer/operation/bulk.rs`
- Modify: `crates/things-mcp/src/core/writer/operation/mod.rs`
- Modify: `crates/things-mcp/src/core/writer/writer.rs`
- Modify: `crates/things-mcp/src/tools/todos.rs`

- [ ] **Step 1: Create `operation/bulk.rs`**

```rust
//! `BulkRawSpec` — pass an arbitrary JSON array of Things URL scheme
//! operation objects straight through `build_url`. Power tool, intended
//! for callers that already have well-formed Things payloads.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BulkRawSpec {
    /// Each element is rendered straight into the data[] array of the URL
    /// payload. The chassis does NOT validate per-element structure.
    pub operations: Vec<Value>,
}

/// The bulk variant doesn't fit the "one operation = one JSON element" model
/// the rest of the enum uses. The render_json method returns the FIRST element
/// to keep the trait shape uniform; for the full payload, callers must
/// extract `BulkRawSpec.operations` directly. In practice, this is invisible
/// because `build_url` is also extended to special-case the BulkRaw variant.
///
/// We choose this shape because the Operation enum is intentionally narrow
/// (one variant = one rendered JSON object); making render_json return Value
/// for non-bulk variants and Vec<Value> for bulk would force every caller
/// into a match. Keeping the trait uniform and special-casing in build_url
/// is the lesser evil — and the build_url change is one if-let.
pub(crate) fn render_bulk_first(spec: &BulkRawSpec) -> Value {
    spec.operations
        .first()
        .cloned()
        .unwrap_or_else(|| Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::writer::operation::Operation;

    #[test]
    fn bulk_action_name_and_auth_requirement() {
        let op = Operation::BulkRaw(BulkRawSpec {
            operations: vec![serde_json::json!({"type": "to-do", "attributes": {"title": "x"}})],
        });
        assert_eq!(op.action_name(), "bulk_json");
        // Bulk is conservatively gated as IF it needs auth — the chassis
        // can't introspect the payload, so it errs on the safe side: requires
        // the token to be present if it was configured. The Writer::fire
        // logic gates this differently than other ops; see the auth-gate
        // discussion in core/writer/writer.rs.
        assert!(op.requires_auth_token());
    }

    #[test]
    fn bulk_render_returns_first_element() {
        // render_json on bulk returns the first element. The complete batch
        // is composed by build_url, not by Operation::render_json.
        let op = Operation::BulkRaw(BulkRawSpec {
            operations: vec![
                serde_json::json!({"type": "to-do", "attributes": {"title": "A"}}),
                serde_json::json!({"type": "to-do", "attributes": {"title": "B"}}),
            ],
        });
        let v = op.render_json();
        assert_eq!(v["attributes"]["title"], "A");
    }
}
```

- [ ] **Step 2: Wire `BulkRaw` into `operation/mod.rs`**

```rust
pub mod add_project;
pub mod add_todo;
pub mod bulk;
pub mod move_todo;
pub mod status_change;
pub mod update_project;
pub mod update_todo;

pub use add_project::AddProjectSpec;
pub use add_todo::AddTodoSpec;
pub use bulk::BulkRawSpec;
pub use move_todo::MoveTodoSpec;
pub use update_project::UpdateProjectSpec;
pub use update_todo::UpdateTodoSpec;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Operation {
    AddTodo(AddTodoSpec),
    AddProject(AddProjectSpec),
    UpdateTodo(UpdateTodoSpec),
    UpdateProject(UpdateProjectSpec),
    CompleteTodo { id: String },
    CancelTodo { id: String },
    MoveTodo(MoveTodoSpec),
    BulkRaw(BulkRawSpec),
}

impl Operation {
    pub fn action_name(&self) -> &'static str {
        match self {
            Operation::AddTodo(_) => "add_todo",
            Operation::AddProject(_) => "add_project",
            Operation::UpdateTodo(_) => "update_todo",
            Operation::UpdateProject(_) => "update_project",
            Operation::CompleteTodo { .. } => "complete_todo",
            Operation::CancelTodo { .. } => "cancel_todo",
            Operation::MoveTodo(_) => "move_todo",
            Operation::BulkRaw(_) => "bulk_json",
        }
    }

    pub fn requires_auth_token(&self) -> bool {
        match self {
            Operation::AddTodo(_) => false,
            Operation::AddProject(_) => false,
            Operation::UpdateTodo(_) => true,
            Operation::UpdateProject(_) => true,
            Operation::CompleteTodo { .. } => true,
            Operation::CancelTodo { .. } => true,
            Operation::MoveTodo(_) => true,
            // Conservative: bulk may carry update operations, and the chassis
            // can't introspect the payload. Demand the token if present;
            // the Writer's auth gate will only fire if no token is configured.
            Operation::BulkRaw(_) => true,
        }
    }

    pub fn render_json(&self) -> Value {
        match self {
            Operation::AddTodo(spec) => add_todo::render_add_todo(spec),
            Operation::AddProject(spec) => add_project::render_add_project(spec),
            Operation::UpdateTodo(spec) => update_todo::render_update_todo(spec),
            Operation::UpdateProject(spec) => update_project::render_update_project(spec),
            Operation::CompleteTodo { id } => status_change::render_complete_todo(id),
            Operation::CancelTodo { id } => status_change::render_cancel_todo(id),
            Operation::MoveTodo(spec) => move_todo::render_move_todo(spec),
            Operation::BulkRaw(spec) => bulk::render_bulk_first(spec),
        }
    }

    /// Returns the full batch as multiple JSON elements. For non-bulk variants,
    /// this is a single-element vec wrapping `render_json()`. For `BulkRaw`,
    /// the entire `operations` vec is returned. `build_url` uses this to
    /// compose the URL's payload array.
    pub fn render_batch(&self) -> Vec<Value> {
        match self {
            Operation::BulkRaw(spec) => spec.operations.clone(),
            _ => vec![self.render_json()],
        }
    }
}
```

- [ ] **Step 3: Update `build_url` to use `render_batch`**

Edit `crates/things-mcp/src/core/writer/url.rs`. Replace the payload assembly inside `build_url`:

The current line is:

```rust
    let payload: Vec<_> = ops.iter().map(|op| op.render_json()).collect();
```

Replace with:

```rust
    let payload: Vec<_> = ops.iter().flat_map(|op| op.render_batch()).collect();
```

This expands `BulkRaw` into its full element list while keeping the single-element behavior for every other variant.

- [ ] **Step 4: Change `Writer::fire` signature to `Option<VerifyPredicate>`**

Edit `crates/things-mcp/src/core/writer/writer.rs`.

Change the `fire` signature:

```rust
    pub async fn fire(
        &self,
        op: Operation,
        verify_pred: Option<VerifyPredicate>,
    ) -> Result<WriteOutcome, ThingsError> {
```

Replace the verify + outcome composition (steps 7–8 in the existing implementation) with:

```rust
        // 6. Open URL via the injected executor.
        let started = Instant::now();
        self.executor.open(&url).await?;

        // 7. Verify by polling the reader (or skip if None).
        let Some(pred) = verify_pred else {
            // No verify predicate → bulk path. Return success-with-verified=false
            // immediately after the executor call.
            let latency_ms = started.elapsed().as_millis() as u64;
            return Ok(WriteOutcome {
                id: None,
                action: op.action_name().to_string(),
                verified: false,
                dry_run: false,
                latency_ms,
            });
        };

        let outcome = verify(
            &self.pool,
            pred,
            self.cfg.poll_timeout,
            self.cfg.poll_interval,
        )
        .await?;

        // 8. Compose outcome.
        let latency_ms = started.elapsed().as_millis() as u64;
        Ok(match outcome {
            VerifyOutcome::Verified { row, .. } => WriteOutcome {
                id: Some(row.id),
                action: op.action_name().to_string(),
                verified: true,
                dry_run: false,
                latency_ms,
            },
            VerifyOutcome::Timeout { .. } | VerifyOutcome::NotFound { .. } => WriteOutcome {
                id: None,
                action: op.action_name().to_string(),
                verified: false,
                dry_run: false,
                latency_ms,
            },
        })
    }
}
```

(Note: the `Timeout | NotFound` arms collapse here — they were two separate identical arms in Plan 4 with the same body. With the new `Option<VerifyPredicate>` shape, collapsing is the natural pairing.)

Update the existing three writer unit tests to wrap their predicate in `Some(...)`. In the test module, edit the three `writer.fire(add_op(...), pred(...))` call sites:

- `fire_returns_test_db_write_forbidden_in_forbidden_mode`: `writer.fire(add_op("anything"), Some(pred("anything"))).await`
- `fire_dry_run_short_circuits_without_calling_executor`: `writer.fire(add_op("Pretend to buy bread"), Some(pred("Pretend to buy bread"))).await`
- `fire_live_calls_executor_then_times_out_against_test_db`: `writer.fire(add_op("Definitely-not-in-fixture row"), Some(pred("Definitely-not-in-fixture row"))).await`

Add a new test for the `None` path. Append inside the `#[cfg(test)] mod tests` block:

```rust
    #[tokio::test]
    async fn fire_with_none_verify_pred_skips_verify_and_returns_unverified() {
        let (_tmp, writer, exec) = build_writer(SafetyMode::Live).await;
        let bulk_op = Operation::BulkRaw(crate::core::writer::operation::BulkRawSpec {
            operations: vec![serde_json::json!({
                "type": "to-do",
                "attributes": {"title": "Anything"}
            })],
        });
        let started = std::time::Instant::now();
        let out = writer.fire(bulk_op, None).await.unwrap();
        // Executor called once.
        let urls = exec.urls();
        assert_eq!(urls.len(), 1);
        // No verify polling — should return well before the configured 200ms timeout.
        assert!(
            started.elapsed() < std::time::Duration::from_millis(150),
            "fire(None) must skip verify and return promptly; elapsed: {:?}",
            started.elapsed()
        );
        // Outcome: unverified (no predicate to verify against), not dry-run.
        assert!(!out.verified);
        assert!(!out.dry_run);
        assert_eq!(out.action, "bulk_json");
        assert!(out.id.is_none());
    }
```

- [ ] **Step 5: Update `things_add_todo` to pass `Some(pred)`**

Edit `crates/things-mcp/src/tools/todos.rs`. Find the existing call:

```rust
    let outcome = state.writer.fire(op, predicate).await?;
```

Replace with:

```rust
    let outcome = state.writer.fire(op, Some(predicate)).await?;
```

- [ ] **Step 6: Build + full sweep**

```
cargo build
cargo test
```

Expected: **104 total** (99 lib + 5 integration). +3 over T6: 2 bulk render/action tests + 1 fire(None) writer test.

- [ ] **Step 7: Commit**

```bash
git add crates/things-mcp/src/core/writer/operation/bulk.rs \
        crates/things-mcp/src/core/writer/operation/mod.rs \
        crates/things-mcp/src/core/writer/url.rs \
        crates/things-mcp/src/core/writer/writer.rs \
        crates/things-mcp/src/tools/todos.rs
git commit -m "core/writer: BulkRaw variant + fire takes Option<VerifyPredicate>"
```

---

### Task 8: 7 tool adapters + server registrations + integration tests

The chassis is complete. Now fan out: write the 7 tool adapter functions, register them on `ThingsServer`, and ship a single integration test file that exercises each end-to-end. Seven tests run in dry-run mode against the test DB; one extra test runs in Live mode against the fixture DB to prove the auth-token survives URL construction and percent-encoding all the way to the recording executor — the first end-to-end exercise of the auth path the spec promised.

**Files:**
- Modify: `crates/things-mcp/src/tools/projects.rs`
- Modify: `crates/things-mcp/src/tools/todos.rs`
- Create: `crates/things-mcp/src/tools/bulk.rs`
- Modify: `crates/things-mcp/src/tools/mod.rs` (declare `pub mod bulk;`)
- Modify: `crates/things-mcp/src/server.rs`
- Create: `crates/things-mcp/tests/end_to_end_writes_plan_5.rs`

- [ ] **Step 1: Inspect `tools/mod.rs` to confirm the module declaration shape**

Read `crates/things-mcp/src/tools/mod.rs`. It currently lists the existing tool modules (`lists`, `projects`, `search`, `todos`). Add a new line at the end:

```rust
pub mod bulk;
```

(Preserve alphabetical ordering if the existing block is alphabetical; in this case `bulk` belongs near the top.)

- [ ] **Step 2: Extend `tools/projects.rs`**

Append at the end of `crates/things-mcp/src/tools/projects.rs`:

```rust
use std::time::{SystemTime, UNIX_EPOCH};

use crate::core::writer::operation::{AddProjectSpec, Operation, UpdateProjectSpec};
use crate::core::writer::outcome::WriteOutcome;
use crate::core::writer::verify::VerifyPredicate;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct AddProjectArgs {
    /// Project title. Required, non-empty.
    pub title: String,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub when: Option<String>,
    #[serde(default)]
    pub deadline: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Parent area UUID.
    #[serde(default)]
    pub area_id: Option<String>,
    /// Initial heading titles. Order preserved.
    #[serde(default)]
    pub headings: Vec<String>,
}

pub async fn things_add_project(
    state: AppState,
    args: AddProjectArgs,
) -> anyhow::Result<WriteOutcome> {
    if args.title.trim().is_empty() {
        return Err(crate::core::error::ThingsError::InvalidInput {
            field: "title".into(),
            reason: "title must be non-empty".into(),
        }
        .into());
    }
    let since_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    let op = Operation::AddProject(AddProjectSpec {
        title: args.title.clone(),
        notes: args.notes,
        when: args.when,
        deadline: args.deadline,
        tags: args.tags,
        area_id: args.area_id,
        todos: Vec::new(),
        headings: args.headings,
    });
    let predicate = VerifyPredicate::CreateByTitle {
        title: args.title,
        since_unix,
        kind: crate::core::types::TaskKind::Project,
    };
    let outcome = state.writer.fire(op, Some(predicate)).await?;
    Ok(outcome)
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct UpdateProjectArgs {
    /// UUID of the project to update.
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub when: Option<String>,
    #[serde(default)]
    pub deadline: Option<String>,
    /// `None` = leave tags unchanged. `Some(vec![])` = clear all tags.
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub area_id: Option<String>,
    #[serde(default)]
    pub completed: Option<bool>,
    #[serde(default)]
    pub canceled: Option<bool>,
}

pub async fn things_update_project(
    state: AppState,
    args: UpdateProjectArgs,
) -> anyhow::Result<WriteOutcome> {
    if args.id.trim().is_empty() {
        return Err(crate::core::error::ThingsError::InvalidInput {
            field: "id".into(),
            reason: "id must be non-empty".into(),
        }
        .into());
    }
    let op = Operation::UpdateProject(UpdateProjectSpec {
        id: args.id.clone(),
        title: args.title.clone(),
        notes: args.notes.clone(),
        when: args.when,
        deadline: args.deadline,
        tags: args.tags,
        area_id: args.area_id,
        completed: args.completed,
        canceled: args.canceled,
    });
    let predicate = VerifyPredicate::UpdateById {
        id: args.id,
        expected_title: args.title,
        expected_notes: args.notes,
    };
    let outcome = state.writer.fire(op, Some(predicate)).await?;
    Ok(outcome)
}
```

(Note: `things_update_project` uses `VerifyPredicate::UpdateById` because the row's id stays the same and the existing predicate already covers title/notes verification. The variant doesn't carry a `kind` field — UpdateById doesn't filter on type, so updating a project (type=1) works the same as updating a to-do (type=0).)

- [ ] **Step 3: Extend `tools/todos.rs`**

Append at the end of `crates/things-mcp/src/tools/todos.rs`:

```rust
use crate::core::writer::operation::{MoveTodoSpec, UpdateTodoSpec};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct UpdateTodoArgs {
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub when: Option<String>,
    #[serde(default)]
    pub deadline: Option<String>,
    /// `None` = leave tags unchanged. `Some(vec![])` = clear all tags.
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub list_id: Option<String>,
    #[serde(default)]
    pub completed: Option<bool>,
    #[serde(default)]
    pub canceled: Option<bool>,
}

pub async fn things_update_todo(
    state: AppState,
    args: UpdateTodoArgs,
) -> anyhow::Result<WriteOutcome> {
    if args.id.trim().is_empty() {
        return Err(crate::core::error::ThingsError::InvalidInput {
            field: "id".into(),
            reason: "id must be non-empty".into(),
        }
        .into());
    }
    let op = Operation::UpdateTodo(UpdateTodoSpec {
        id: args.id.clone(),
        title: args.title.clone(),
        notes: args.notes.clone(),
        when: args.when,
        deadline: args.deadline,
        tags: args.tags,
        list_id: args.list_id,
        completed: args.completed,
        canceled: args.canceled,
    });
    let predicate = VerifyPredicate::UpdateById {
        id: args.id,
        expected_title: args.title,
        expected_notes: args.notes,
    };
    let outcome = state.writer.fire(op, Some(predicate)).await?;
    Ok(outcome)
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct StatusChangeArgs {
    /// UUID of the to-do to mark completed or canceled.
    pub id: String,
}

pub async fn things_complete_todo(
    state: AppState,
    args: StatusChangeArgs,
) -> anyhow::Result<WriteOutcome> {
    if args.id.trim().is_empty() {
        return Err(crate::core::error::ThingsError::InvalidInput {
            field: "id".into(),
            reason: "id must be non-empty".into(),
        }
        .into());
    }
    let op = Operation::CompleteTodo { id: args.id.clone() };
    let predicate = VerifyPredicate::StatusChange {
        id: args.id,
        want: crate::core::types::TaskStatus::Completed,
    };
    let outcome = state.writer.fire(op, Some(predicate)).await?;
    Ok(outcome)
}

pub async fn things_cancel_todo(
    state: AppState,
    args: StatusChangeArgs,
) -> anyhow::Result<WriteOutcome> {
    if args.id.trim().is_empty() {
        return Err(crate::core::error::ThingsError::InvalidInput {
            field: "id".into(),
            reason: "id must be non-empty".into(),
        }
        .into());
    }
    let op = Operation::CancelTodo { id: args.id.clone() };
    let predicate = VerifyPredicate::StatusChange {
        id: args.id,
        want: crate::core::types::TaskStatus::Canceled,
    };
    let outcome = state.writer.fire(op, Some(predicate)).await?;
    Ok(outcome)
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct MoveTodoArgs {
    /// UUID of the to-do to move.
    pub id: String,
    /// Target project or area UUID. `None` (omitted) moves to the Inbox.
    #[serde(default)]
    pub list_id: Option<String>,
}

pub async fn things_move_todo(
    state: AppState,
    args: MoveTodoArgs,
) -> anyhow::Result<WriteOutcome> {
    if args.id.trim().is_empty() {
        return Err(crate::core::error::ThingsError::InvalidInput {
            field: "id".into(),
            reason: "id must be non-empty".into(),
        }
        .into());
    }
    let op = Operation::MoveTodo(MoveTodoSpec {
        id: args.id.clone(),
        list_id: args.list_id.clone(),
    });
    let predicate = VerifyPredicate::MoveById {
        id: args.id,
        expected_list_id: args.list_id,
    };
    let outcome = state.writer.fire(op, Some(predicate)).await?;
    Ok(outcome)
}
```

- [ ] **Step 4: Create `tools/bulk.rs`**

`crates/things-mcp/src/tools/bulk.rs`:

```rust
//! Bulk write tool. Pipes a raw JSON array of Things URL scheme operation
//! objects through `build_url` and the executor without per-element
//! verification. Power tool — described as destructive in MCP annotations
//! because the chassis cannot reason about what the LLM is asking Things
//! to do.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::core::writer::operation::{BulkRawSpec, Operation};
use crate::core::writer::outcome::WriteOutcome;
use crate::state::AppState;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct BulkJsonArgs {
    /// Things URL scheme operation objects. Each element must be a JSON
    /// object — primitives or arrays are rejected. Max 250 elements per
    /// the Things rate-limit guidance.
    pub operations: Vec<serde_json::Value>,
}

pub async fn things_bulk_json(
    state: AppState,
    args: BulkJsonArgs,
) -> anyhow::Result<WriteOutcome> {
    // Pre-condition: non-empty, within rate limit.
    if args.operations.is_empty() {
        return Err(crate::core::error::ThingsError::InvalidInput {
            field: "operations".into(),
            reason: "operations must be non-empty".into(),
        }
        .into());
    }
    if args.operations.len() > 250 {
        return Err(crate::core::error::ThingsError::InvalidInput {
            field: "operations".into(),
            reason: format!(
                "operations exceeds Things rate limit (max 250, got {})",
                args.operations.len()
            ),
        }
        .into());
    }
    // Pre-condition: each element must be a JSON object.
    for (i, op) in args.operations.iter().enumerate() {
        if !op.is_object() {
            return Err(crate::core::error::ThingsError::InvalidInput {
                field: format!("operations[{i}]"),
                reason: "each element must be a JSON object".into(),
            }
            .into());
        }
    }
    let op = Operation::BulkRaw(BulkRawSpec {
        operations: args.operations,
    });
    // No verify predicate — bulk is fire-and-forget. The WriteOutcome.verified
    // field is always false; callers needing strict verification should use
    // the individual tools.
    let outcome = state.writer.fire(op, None).await?;
    Ok(outcome)
}
```

- [ ] **Step 5: Register all 7 new tools on `ThingsServer`**

Edit `crates/things-mcp/src/server.rs`.

Update the existing `use crate::tools::todos::...` line (currently `things_add_todo, things_get_todo, AddTodoArgs, GetTodoArgs`):

```rust
use crate::tools::todos::{
    things_add_todo, things_cancel_todo, things_complete_todo, things_get_todo,
    things_move_todo, things_update_todo,
    AddTodoArgs, GetTodoArgs, MoveTodoArgs, StatusChangeArgs, UpdateTodoArgs,
};
```

Update the existing `use crate::tools::projects::...` line (currently `things_get_project, GetProjectArgs`):

```rust
use crate::tools::projects::{
    things_add_project, things_get_project, things_update_project,
    AddProjectArgs, GetProjectArgs, UpdateProjectArgs,
};
```

Add a new `use`:

```rust
use crate::tools::bulk::{things_bulk_json, BulkJsonArgs};
```

Inside the `#[tool_router] impl ThingsServer { ... }` block, just before the closing `}` (so just after the existing `tool_add_todo`), insert the 7 new `#[tool]` declarations:

```rust
    #[tool(
        name = "things_add_project",
        description = "Create a new project in Things, optionally with initial headings nested inside. Returns a WriteOutcome with the new id once verified.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn tool_add_project(
        &self,
        Parameters(args): Parameters<AddProjectArgs>,
    ) -> Result<Json<WriteOutcome>, McpError> {
        let out = things_add_project(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(out))
    }

    #[tool(
        name = "things_update_todo",
        description = "Update an existing to-do's title, notes, scheduling, tags, list, or status. Only populated fields are sent. Requires the Things auth-token.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn tool_update_todo(
        &self,
        Parameters(args): Parameters<UpdateTodoArgs>,
    ) -> Result<Json<WriteOutcome>, McpError> {
        let out = things_update_todo(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(out))
    }

    #[tool(
        name = "things_update_project",
        description = "Update an existing project's title, notes, scheduling, tags, parent area, or status. Only populated fields are sent. Requires the Things auth-token.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn tool_update_project(
        &self,
        Parameters(args): Parameters<UpdateProjectArgs>,
    ) -> Result<Json<WriteOutcome>, McpError> {
        let out = things_update_project(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(out))
    }

    #[tool(
        name = "things_complete_todo",
        description = "Mark a to-do as completed. Idempotent: re-completing has no further effect. Requires the Things auth-token.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    async fn tool_complete_todo(
        &self,
        Parameters(args): Parameters<StatusChangeArgs>,
    ) -> Result<Json<WriteOutcome>, McpError> {
        let out = things_complete_todo(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(out))
    }

    #[tool(
        name = "things_cancel_todo",
        description = "Mark a to-do as canceled (distinct from completed in Things). Idempotent. Requires the Things auth-token.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    async fn tool_cancel_todo(
        &self,
        Parameters(args): Parameters<StatusChangeArgs>,
    ) -> Result<Json<WriteOutcome>, McpError> {
        let out = things_cancel_todo(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(out))
    }

    #[tool(
        name = "things_move_todo",
        description = "Move a to-do under a project, area, or to the Inbox (when list_id is omitted). Requires the Things auth-token.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn tool_move_todo(
        &self,
        Parameters(args): Parameters<MoveTodoArgs>,
    ) -> Result<Json<WriteOutcome>, McpError> {
        let out = things_move_todo(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(out))
    }

    #[tool(
        name = "things_bulk_json",
        description = "Power tool: send a raw array of Things JSON URL scheme operation objects. Max 250 elements. No per-element verification — WriteOutcome.verified is always false. Use individual tools when verification matters.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn tool_bulk_json(
        &self,
        Parameters(args): Parameters<BulkJsonArgs>,
    ) -> Result<Json<WriteOutcome>, McpError> {
        let out = things_bulk_json(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(out))
    }
```

- [ ] **Step 6: Build to confirm the wiring**

```
cargo build
```

Expected: clean. (Tests haven't been added yet, but the build must succeed.)

- [ ] **Step 7: Create the integration test file**

`crates/things-mcp/tests/end_to_end_writes_plan_5.rs`:

```rust
//! End-to-end exercise of every Plan-5 write tool in dry-run mode. Each test
//! constructs an AppState with the test-DB safety gate set to DryRun and a
//! RecordingExecutor injected. Because dry-run short-circuits before the
//! executor is called, every test asserts `recorder.urls().is_empty()` plus
//! `out.dry_run == true`.
//!
//! ONE exception: the `update_todo_includes_auth_token_when_configured` test
//! switches to Live mode and asserts the recorded URL contains the
//! percent-encoded auth-token. This is the first end-to-end exercise of the
//! auth path.

use std::sync::Arc;

use things_mcp::core::reader::fixture::build_fixture;
use things_mcp::core::writer::executor::{Executor, RecordingExecutor};
use things_mcp::state::{AppState, AppStateOptions};
use things_mcp::tools::bulk::{things_bulk_json, BulkJsonArgs};
use things_mcp::tools::projects::{
    things_add_project, things_update_project, AddProjectArgs, UpdateProjectArgs,
};
use things_mcp::tools::todos::{
    things_cancel_todo, things_complete_todo, things_move_todo, things_update_todo,
    MoveTodoArgs, StatusChangeArgs, UpdateTodoArgs,
};

async fn build_dryrun_state(
    recorder: Arc<dyn Executor>,
) -> AppState {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("things.sqlite");
    build_fixture(&db).unwrap();
    let state = AppState::build(AppStateOptions {
        env_db_path: Some(db.clone()),
        home_dir: tmp.path().to_path_buf(),
        config_path: tmp.path().join("config.toml"),
        allow_writes_on_test_db: true,
        executor_override: Some(recorder),
    })
    .await
    .unwrap();
    std::mem::forget(tmp);
    state
}

#[tokio::test]
async fn add_project_dry_run_does_not_call_executor() {
    let recorder = Arc::new(RecordingExecutor::new());
    let state = build_dryrun_state(recorder.clone() as Arc<dyn Executor>).await;
    let out = things_add_project(
        state,
        AddProjectArgs {
            title: "Launch website".into(),
            area_id: Some("area-2".into()),
            headings: vec!["Design".into(), "QA".into()],
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(out.dry_run);
    assert_eq!(out.action, "add_project");
    assert!(recorder.urls().is_empty());
}

#[tokio::test]
async fn update_todo_dry_run_does_not_call_executor() {
    let recorder = Arc::new(RecordingExecutor::new());
    let state = build_dryrun_state(recorder.clone() as Arc<dyn Executor>).await;
    let out = things_update_todo(
        state,
        UpdateTodoArgs {
            id: "todo-1".into(),
            title: Some("Buy oat milk".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(out.dry_run);
    assert_eq!(out.action, "update_todo");
    assert!(recorder.urls().is_empty());
}

#[tokio::test]
async fn update_project_dry_run_does_not_call_executor() {
    let recorder = Arc::new(RecordingExecutor::new());
    let state = build_dryrun_state(recorder.clone() as Arc<dyn Executor>).await;
    let out = things_update_project(
        state,
        UpdateProjectArgs {
            id: "proj-1".into(),
            title: Some("Reading list — 2026".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(out.dry_run);
    assert_eq!(out.action, "update_project");
    assert!(recorder.urls().is_empty());
}

#[tokio::test]
async fn complete_todo_dry_run_does_not_call_executor() {
    let recorder = Arc::new(RecordingExecutor::new());
    let state = build_dryrun_state(recorder.clone() as Arc<dyn Executor>).await;
    let out = things_complete_todo(
        state,
        StatusChangeArgs { id: "todo-1".into() },
    )
    .await
    .unwrap();
    assert!(out.dry_run);
    assert_eq!(out.action, "complete_todo");
    assert!(recorder.urls().is_empty());
}

#[tokio::test]
async fn cancel_todo_dry_run_does_not_call_executor() {
    let recorder = Arc::new(RecordingExecutor::new());
    let state = build_dryrun_state(recorder.clone() as Arc<dyn Executor>).await;
    let out = things_cancel_todo(
        state,
        StatusChangeArgs { id: "todo-1".into() },
    )
    .await
    .unwrap();
    assert!(out.dry_run);
    assert_eq!(out.action, "cancel_todo");
    assert!(recorder.urls().is_empty());
}

#[tokio::test]
async fn move_todo_dry_run_does_not_call_executor() {
    let recorder = Arc::new(RecordingExecutor::new());
    let state = build_dryrun_state(recorder.clone() as Arc<dyn Executor>).await;
    let out = things_move_todo(
        state,
        MoveTodoArgs {
            id: "todo-1".into(),
            list_id: Some("proj-1".into()),
        },
    )
    .await
    .unwrap();
    assert!(out.dry_run);
    assert_eq!(out.action, "move_todo");
    assert!(recorder.urls().is_empty());
}

#[tokio::test]
async fn bulk_json_dry_run_does_not_call_executor() {
    let recorder = Arc::new(RecordingExecutor::new());
    let state = build_dryrun_state(recorder.clone() as Arc<dyn Executor>).await;
    let out = things_bulk_json(
        state,
        BulkJsonArgs {
            operations: vec![
                serde_json::json!({
                    "type": "to-do",
                    "attributes": { "title": "Bulk A" }
                }),
                serde_json::json!({
                    "type": "to-do",
                    "attributes": { "title": "Bulk B" }
                }),
            ],
        },
    )
    .await
    .unwrap();
    assert!(out.dry_run);
    assert_eq!(out.action, "bulk_json");
    assert!(recorder.urls().is_empty());
}

#[tokio::test]
async fn update_todo_includes_auth_token_in_url_when_configured() {
    // End-to-end auth-token exercise. We need Live mode (so the dry-run gate
    // doesn't short-circuit before the executor) but with a fixture DB (so
    // we don't touch the user's real Things). Live mode is the default when
    // `env_db_path` is `None`; the fixture path is resolved out of
    // `cfg.things.db_path` from config.toml.
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
auth_token = "test-token-123"

[writer]
poll_timeout_ms = 100
poll_interval_ms = 10
"#,
            db.display(),
        ),
    )
    .unwrap();
    let recorder = Arc::new(RecordingExecutor::new());
    let state = AppState::build(AppStateOptions {
        env_db_path: None,  // Live mode — auth-token is loaded from config.toml
        home_dir: tmp.path().to_path_buf(),
        config_path: config_toml,
        allow_writes_on_test_db: false,
        executor_override: Some(recorder.clone() as Arc<dyn Executor>),
    })
    .await
    .unwrap();
    std::mem::forget(tmp);

    let out = things_update_todo(
        state,
        UpdateTodoArgs {
            id: "todo-1".into(),
            title: Some("Buy oat milk".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // The fixture has no live Things app, so verify will time out. That's
    // fine — what we're proving is that the URL was constructed AND the
    // auth-token segment was included.
    assert!(!out.dry_run);
    assert!(!out.verified);
    assert_eq!(out.action, "update_todo");
    let urls = recorder.urls();
    assert_eq!(urls.len(), 1, "executor should record exactly one URL");
    // The literal token survives percent-encoding because it contains only
    // unreserved characters (alpha + digit + hyphen).
    assert!(
        urls[0].contains("&auth-token=test-token-123"),
        "URL should carry the percent-encoded auth-token; got: {}",
        urls[0],
    );
}
```

- [ ] **Step 8: Build + full sweep**

```
cargo build
cargo test
```

Expected: **112 total** (99 lib + 13 integration). +8 over T7: 7 dry-run tests + 1 Live-mode auth-token test.

- [ ] **Step 9: Commit**

```bash
git add crates/things-mcp/src/tools/projects.rs \
        crates/things-mcp/src/tools/todos.rs \
        crates/things-mcp/src/tools/bulk.rs \
        crates/things-mcp/src/tools/mod.rs \
        crates/things-mcp/src/server.rs \
        crates/things-mcp/tests/end_to_end_writes_plan_5.rs
git commit -m "tools: 7 plan-5 write tools + dry-run + auth-token e2e tests"
```

---

### Task 9: README + final sweep

Bump the README status line. Confirm `cargo test` shows 111 passing and `cargo build --release` is clean.

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Bump the status line**

Open `README.md`. The current status line (from Plan 4) is:

```markdown
**Status:** Plan 4 — read surface complete + first write tool (`things_add_todo`) shipping over the JSON URL scheme. Writes go through `core/writer/`: typed `Operation` → percent-encoded URL → `/usr/bin/open -g` (or injected test executor) → bounded poll against the SQLite reader → `WriteOutcome`. Test-DB mode short-circuits to dry-run; auth-token (required only for updates) wired but not yet exercised. See `docs/superpowers/plans/` for the active plan and follow-ons.
```

Replace with:

```markdown
**Status:** Plan 5 — full write surface shipping over the JSON URL scheme: `things_add_todo`, `things_add_project`, `things_update_todo`, `things_update_project`, `things_complete_todo`, `things_cancel_todo`, `things_move_todo`, and the `things_bulk_json` power tool. Updates flow through the auth-token gate (`THINGS_AUTH_TOKEN` env or `[things].auth_token` in `config.toml`). Bulk skips per-element verify; all other tools poll the reader for a typed predicate (`CreateByTitle`, `UpdateById`, `StatusChange`, `MoveById`) up to `writer.poll_timeout_ms`. See `docs/superpowers/plans/` for the active plan and follow-ons.
```

- [ ] **Step 2: Full sweep + release build**

```
cargo test && cargo build --release
```

Expected: **112 tests pass**; release build clean. The `#[ignore]` smoke test stays ignored.

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: README — plan 5 full write surface shipping"
```

- [ ] **Step 4: Inspect history**

```
git log --oneline | head -12
```

Expected: 9 new commits on top of `cd5a0bb` (one per task in this plan).

---

## Self-review checklist (for the executor)

- [ ] All 7 new tools (`add_project`, `update_todo`, `update_project`, `complete_todo`, `cancel_todo`, `move_todo`, `bulk_json`) are registered on `ThingsServer` with the MCP annotations from the spec's §1 table.
- [ ] `things_bulk_json` has `destructive_hint = true`; the four update/move/add tools have `destructive_hint = false`.
- [ ] `things_complete_todo` and `things_cancel_todo` have `idempotent_hint = true`; all other writes have `idempotent_hint = false`.
- [ ] `core/writer/operation/` contains 8 files (mod, add_todo, add_project, update_todo, update_project, status_change, move_todo, bulk); no file exceeds ~250 lines.
- [ ] The original `core/writer/operation.rs` file is deleted.
- [ ] `VerifyPredicate::CreateByTitle` carries a `kind: TaskKind` field; the verify SQL parameterises on `t.type`.
- [ ] `VerifyPredicate::MoveById` exists; the existence-probe at the top of `verify()` includes it in the OR-pattern.
- [ ] `Writer::fire`'s second argument is `Option<VerifyPredicate>`; the `None` branch composes a `WriteOutcome { verified: false }` without calling `verify()`.
- [ ] `Operation::BulkRaw.requires_auth_token()` returns `true` (conservative; chassis can't introspect the payload).
- [ ] `Operation::render_batch()` exists; `build_url` calls it via `flat_map` so bulk expands its payload while every other variant remains single-element.
- [ ] All Plan-4 tests still pass (no regressions in `core::writer::operation::add_todo::tests`, `core::writer::verify::tests`, `core::writer::writer::tests`, `end_to_end_add_todo`).
- [ ] No new dependencies in `Cargo.toml`. No new variants on `ThingsError`.
- [ ] Every new tool's adapter function pre-validates non-empty id/title and rejects with `ThingsError::InvalidInput` if empty.
- [ ] `things_bulk_json` rejects empty `operations` and arrays > 250 elements; also rejects elements that are not JSON objects.
- [ ] The `update_todo_includes_auth_token_in_url_when_configured` integration test passes — the URL recorded by the RecordingExecutor contains `&auth-token=test-token-123`.
- [ ] `cargo test` shows **112 tests pass** at the end of Task 9; `cargo build --release` is clean.

When all green, the natural next step is **Plan 6** (tag CRUD via the AppleScript wrapper). Plan 5's chassis (`core/writer/`) is reused unchanged.
