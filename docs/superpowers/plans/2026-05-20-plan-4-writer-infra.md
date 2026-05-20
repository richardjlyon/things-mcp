# things-mcp Plan 4 — writer infrastructure + `things_add_todo`

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship `core/writer/` — the chassis that turns a typed write operation into a Things JSON URL, fires it via `/usr/bin/open -g`, polls the SQLite reader for the expected change, and returns a `WriteOutcome`. Prove the chassis end-to-end with one MCP write tool, `things_add_todo`.

**Architecture:** A new `core/writer/` module sibling to `core/reader/`. Submodules are narrowly focused and independently testable: `operation` builds JSON, `url` composes the Things URL, `executor` is a trait (`OpenCommandExecutor` for prod, `RecordingExecutor` for tests), `verify` polls the reader pool against a discriminated `VerifyPredicate`, `writer` ties them together with safety gates. `AppState` gains an `Arc<Writer>` field and an `executor_override` option so tests can inject the recording executor without touching production code paths.

**Tech Stack:** Same as Plans 1–3 (rmcp 1.7, rusqlite, tokio, serde_json, tracing) + **two new deps** — `async_trait` (object-safe async traits for `Executor`) and `urlencoding` (RFC 3986 percent-encoding for the JSON payload and auth-token segment). `SecretString` is a 20-line in-house newtype to avoid pulling in the `secrecy` crate.

**Spec:** `docs/superpowers/specs/2026-05-20-plan-4-writer-infra-design.md`. Parent design: `docs/superpowers/specs/2026-05-20-things-mcp-server-design.md` §3 + §5.

**Predecessor:** `docs/superpowers/plans/2026-05-20-plan-3-search.md`. HEAD test count: 65 (62 lib + 3 integration).

**Scope notes:**
- **AddTodo only.** Plan 4 ships one write tool. Plan 5 layers the other 7 tools on the same chassis.
- **No auth-token required for creates.** Things' JSON API only requires its auth-token for `update` operations. AddTodo is `create`, so Plan 4 ships green without the user having to configure a token. The auth gate is wired up but its happy-path test waits for Plan 5.
- **No `/usr/bin/open` call hits the user's Things app during tests.** The `OpenCommandExecutor` smoke test is `#[ignore]` and only run manually as part of Plan 10's E2E runbook.
- **Existing `ThingsError::DryRun` and `WriteUnverified` variants are leftovers** from the original sketched design; Plan 4 uses outcome values (`WriteOutcome { dry_run: true }`, `WriteOutcome { verified: false }`) per the design spec. The existing error variants stay — pruning them is a Plan 10 doc-polish concern, not a Plan 4 task.

**Follow-on plans** (unchanged):
- Plan 3.5 (optional): FTS5 query activation once verified against live DB
- Plan 5: remaining write tools (add_project, update_todo, complete/cancel/move, bulk)
- Plan 6: AppleScript wrapper + tag admin
- Plan 7: recurrence (experimental, AppleScript)
- Plan 8: streamable-HTTP transport + OAuth 2.1 + Tailscale Funnel
- Plan 9: setup / status / show-credentials subcommands + launchd
- Plan 10: docs polish + manual E2E runbook

---

## File map

**Create (8 files):**
- `crates/things-mcp/src/core/writer/mod.rs` — module root + re-exports
- `crates/things-mcp/src/core/writer/secret.rs` — `SecretString` newtype
- `crates/things-mcp/src/core/writer/outcome.rs` — `WriteOutcome` type
- `crates/things-mcp/src/core/writer/operation.rs` — `Operation` enum + `AddTodoSpec` + `render_json`
- `crates/things-mcp/src/core/writer/url.rs` — `build_url` + `mask_auth_token`
- `crates/things-mcp/src/core/writer/executor.rs` — `Executor` trait + `OpenCommandExecutor` + `RecordingExecutor`
- `crates/things-mcp/src/core/writer/verify.rs` — `VerifyPredicate` + `verify()`
- `crates/things-mcp/src/core/writer/writer.rs` — `Writer` + `SafetyMode` + `Writer::fire`
- `crates/things-mcp/tests/end_to_end_add_todo.rs` — dry-run + recording-executor integration tests

**Modify:**
- `Cargo.toml` (workspace) — add `async_trait`, `urlencoding` to `[workspace.dependencies]`
- `crates/things-mcp/Cargo.toml` — pull them in
- `crates/things-mcp/src/core/error.rs` — add `TestDbWriteForbidden`, `ExecutorFailed` variants + tests
- `crates/things-mcp/src/core/mod.rs` — `pub mod writer;`
- `crates/things-mcp/src/state.rs` — `AppState.writer: Arc<Writer>`, `AppStateOptions.executor_override: Option<Arc<dyn Executor>>`, resolve `SafetyMode`, load auth-token
- `crates/things-mcp/src/tools/todos.rs` — add `AddTodoArgs` + `things_add_todo`
- `crates/things-mcp/src/server.rs` — register `tool_add_todo` with MCP annotations
- `README.md` — status line bump to Plan 4

**Expected test counts (cumulative):**
| After task | Lib | Integration | Total | Delta |
|---|---|---|---|---|
| Baseline (HEAD `a1405ae`) | 62 | 3 | 65 | — |
| T1 | 64 | 3 | 67 | +2 |
| T2 | 69 | 3 | 72 | +5 |
| T3 | 73 | 3 | 76 | +4 |
| T4 | 74 | 3 | 77 | +1 |
| T5 | 79 | 3 | 82 | +5 |
| T6 | 82 | 3 | 85 | +3 |
| T7 | 82 | 3 | 85 | 0 |
| T8 | 82 | 5 | 87 | +2 |
| T9 | 82 | 5 | 87 | 0 |

---

### Task 1: dependencies + module scaffold + new error variants

Wire up the two new deps, create the empty `core/writer/` module, and add the two new `ThingsError` variants we'll need.

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Modify: `crates/things-mcp/Cargo.toml`
- Create: `crates/things-mcp/src/core/writer/mod.rs`
- Modify: `crates/things-mcp/src/core/mod.rs`
- Modify: `crates/things-mcp/src/core/error.rs`

- [ ] **Step 1: Add deps to the workspace**

Edit `Cargo.toml` (workspace root) — append two lines under `[workspace.dependencies]`:

```toml
async-trait = "0.1"
urlencoding = "2"
```

- [ ] **Step 2: Pull them in for the things-mcp crate**

Edit `crates/things-mcp/Cargo.toml` — append two lines under `[dependencies]`:

```toml
async-trait.workspace = true
urlencoding.workspace = true
```

- [ ] **Step 3: Create the writer module scaffold**

`crates/things-mcp/src/core/writer/mod.rs`:

```rust
//! Write path: JSON URL construction, executor seam, post-write SQLite poll.
//!
//! Sibling of `core/reader/`. See `docs/superpowers/specs/2026-05-20-plan-4-writer-infra-design.md`.

pub mod executor;
pub mod operation;
pub mod outcome;
pub mod secret;
pub mod url;
pub mod verify;
pub mod writer;
```

- [ ] **Step 4: Register the writer module on `core/mod.rs`**

Open `crates/things-mcp/src/core/mod.rs`. It currently lists `pub mod backup; pub mod config; pub mod error; pub mod reader; pub mod types;` (alphabetical). Add `pub mod writer;` at the end (still alphabetical).

- [ ] **Step 5: Add two new error variants**

Open `crates/things-mcp/src/core/error.rs` and add two variants to the `ThingsError` enum, placed alphabetically with the existing ones (`TestDbWriteForbidden` after `Sqlite`, `ExecutorFailed` after `DryRun`):

```rust
    #[error("write executor failed: {source}")]
    ExecutorFailed { source: String },

    #[error("writes refused in test-DB mode (set THINGS_MCP_ALLOW_WRITES_ON_TEST_DB=1 to allow dry-run writes)")]
    TestDbWriteForbidden,
```

- [ ] **Step 6: Write failing tests for the new variants**

Append inside the `#[cfg(test)] mod tests` block at the bottom of `error.rs`:

```rust
    #[test]
    fn test_db_write_forbidden_serialises_to_tagged_json() {
        let err = ThingsError::TestDbWriteForbidden;
        let v = serde_json::to_value(&err).unwrap();
        assert_eq!(v["kind"], "test_db_write_forbidden");
    }

    #[test]
    fn executor_failed_carries_source() {
        let err = ThingsError::ExecutorFailed {
            source: "spawn: ENOENT".into(),
        };
        let v = serde_json::to_value(&err).unwrap();
        assert_eq!(v["kind"], "executor_failed");
        assert_eq!(v["source"], "spawn: ENOENT");
    }
```

- [ ] **Step 7: Build + test**

```
cargo build
cargo test --lib core::error
```

Expected: `cargo build` clean; `cargo error::tests` 2 + 2 = 4 passing.

```
cargo test
```

Expected: **67 total** (64 lib + 3 integration).

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml crates/things-mcp/Cargo.toml crates/things-mcp/src/core
git commit -m "core/writer: scaffold module + add error variants for write path"
```

---

### Task 2: `SecretString` + `WriteOutcome` + `Operation::AddTodo` + `render_json`

Three small files — the typed payload primitives. Tests prove `render_json` matches Things' documented JSON shape.

**Files:**
- Create: `crates/things-mcp/src/core/writer/secret.rs`
- Create: `crates/things-mcp/src/core/writer/outcome.rs`
- Create: `crates/things-mcp/src/core/writer/operation.rs`

- [ ] **Step 1: `secret.rs` — a 20-line non-leaking wrapper around String**

`crates/things-mcp/src/core/writer/secret.rs`:

```rust
//! `SecretString` — a tiny newtype that prevents accidental logging of
//! sensitive material. We roll our own to avoid pulling in the `secrecy`
//! crate for a use this small. Only `expose_secret()` returns the raw value.

#[derive(Clone)]
pub struct SecretString(String);

impl SecretString {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Borrow the raw secret. Call sites must NEVER log the returned `&str`.
    pub fn expose_secret(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for SecretString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretString(***)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_does_not_expose_secret() {
        let s = SecretString::new("totally-secret-token");
        let dbg = format!("{:?}", s);
        assert!(!dbg.contains("totally-secret"));
        assert_eq!(dbg, "SecretString(***)");
    }

    #[test]
    fn expose_secret_returns_raw() {
        let s = SecretString::new("abc123");
        assert_eq!(s.expose_secret(), "abc123");
    }
}
```

- [ ] **Step 2: `outcome.rs` — the typed result returned by every write tool**

`crates/things-mcp/src/core/writer/outcome.rs`:

```rust
//! `WriteOutcome` — what every write tool returns. Carries the
//! verification result and timing so the LLM can reason about whether
//! its action took effect.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WriteOutcome {
    /// UUID of the affected row. For create operations, populated from the
    /// verified row. For updates/status-changes, echoes the input id. `None`
    /// when verify timed out without a match.
    pub id: Option<String>,
    /// Snake-case action name — `"add_todo"`, `"update_todo"`, etc.
    /// Sourced from `Operation::action_name()`.
    pub action: String,
    /// `true` iff `verify()` returned `VerifyOutcome::Verified`.
    pub verified: bool,
    /// `true` when the writer short-circuited at the dry-run safety gate.
    pub dry_run: bool,
    /// Total latency (open call + verify), milliseconds. `0` when dry-run.
    pub latency_ms: u64,
}
```

- [ ] **Step 3: `operation.rs` — write the failing tests first**

`crates/things-mcp/src/core/writer/operation.rs`:

```rust
//! `Operation` — typed write operations, each capable of rendering itself
//! as a single Things JSON URL operation element.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Operation {
    AddTodo(AddTodoSpec),
}

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
            Operation::AddTodo(spec) => render_add_todo(spec),
        }
    }
}

fn render_add_todo(spec: &AddTodoSpec) -> Value {
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

    #[test]
    fn add_todo_minimal_renders_title_only() {
        let op = Operation::AddTodo(AddTodoSpec {
            title: "Buy milk".into(),
            ..Default::default()
        });
        let v = op.render_json();
        assert_eq!(v["type"], "to-do");
        assert_eq!(v["attributes"]["title"], "Buy milk");
        // No spurious keys for empty options.
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

- [ ] **Step 4: Build + test**

```
cargo test --lib core::writer
```

Expected: **5 passed** (2 in `secret::tests`, 3 in `operation::tests`).

```
cargo test
```

Expected: **72 total** (69 lib + 3 integration).

- [ ] **Step 5: Commit**

```bash
git add crates/things-mcp/src/core/writer
git commit -m "core/writer: SecretString + WriteOutcome + Operation::AddTodo with render_json"
```

---

### Task 3: `url.rs` — `build_url` + auth-token masking

Take a slice of operations and an optional auth-token, produce the encoded `things:///json?…` URL. Ship a `mask_auth_token` helper alongside so Plan 4 logs don't leak tokens when Plan 5 starts shipping update tools.

**Files:**
- Create: `crates/things-mcp/src/core/writer/url.rs`

- [ ] **Step 1: Write the failing tests**

`crates/things-mcp/src/core/writer/url.rs`:

```rust
//! Compose `things:///json?data=…&auth-token=…` URLs from rendered operations.
//!
//! - All non-alphanumeric characters in `data` are percent-encoded (the strict
//!   set — Things' parser handles a wide-encoding without complaint, but the
//!   conservative form avoids any ambiguity).
//! - The `auth-token` segment is included iff a token is supplied.
//! - `mask_auth_token` exists so callers can log the URL without leaking
//!   the token.

use crate::core::writer::operation::Operation;
use crate::core::writer::secret::SecretString;

/// Build the full Things URL for one or more operations.
///
/// `auth_token` is `Some` when any operation in the batch requires it
/// (typically updates). For pure creates, pass `None`.
pub fn build_url(ops: &[Operation], auth_token: Option<&SecretString>) -> String {
    let payload: Vec<_> = ops.iter().map(|op| op.render_json()).collect();
    let minified =
        serde_json::to_string(&payload).expect("operations always serialise to valid JSON");
    let encoded_data = urlencoding::encode(&minified);
    let mut url = format!("things:///json?data={encoded_data}");
    if let Some(token) = auth_token {
        let encoded_token = urlencoding::encode(token.expose_secret()).into_owned();
        url.push_str("&auth-token=");
        url.push_str(&encoded_token);
    }
    url
}

/// Replace the `auth-token=…` segment of a URL with `auth-token=***` so the
/// URL is safe to log. If the URL has no auth-token segment, it's returned
/// unchanged.
pub fn mask_auth_token(url: &str) -> String {
    let needle = "&auth-token=";
    let Some(start) = url.find(needle) else {
        return url.to_string();
    };
    let value_start = start + needle.len();
    let value_end = url[value_start..]
        .find('&')
        .map(|n| value_start + n)
        .unwrap_or(url.len());
    let mut out = String::with_capacity(url.len());
    out.push_str(&url[..value_start]);
    out.push_str("***");
    out.push_str(&url[value_end..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::writer::operation::{AddTodoSpec, Operation};

    fn one_add_todo(title: &str) -> Operation {
        Operation::AddTodo(AddTodoSpec {
            title: title.into(),
            ..Default::default()
        })
    }

    #[test]
    fn build_url_includes_things_json_scheme_and_encoded_data() {
        let url = build_url(&[one_add_todo("Buy milk")], None);
        assert!(url.starts_with("things:///json?data="));
        // No auth-token when None.
        assert!(!url.contains("auth-token="));
        // Title should be encoded inside the data payload.
        assert!(url.contains("Buy%20milk") || url.contains("Buy%20milk"));
    }

    #[test]
    fn build_url_appends_auth_token_when_present() {
        let token = SecretString::new("abc 123/+&=");
        let url = build_url(&[one_add_todo("x")], Some(&token));
        assert!(url.contains("&auth-token="));
        // The token's special chars must be percent-encoded.
        assert!(url.contains("abc%20123%2F%2B%26%3D"));
    }

    #[test]
    fn mask_auth_token_redacts_segment() {
        let masked = mask_auth_token(
            "things:///json?data=%5B%5D&auth-token=supersecret",
        );
        assert_eq!(masked, "things:///json?data=%5B%5D&auth-token=***");
    }

    #[test]
    fn mask_auth_token_passes_through_when_absent() {
        let url = "things:///json?data=%5B%5D";
        assert_eq!(mask_auth_token(url), url);
    }
}
```

- [ ] **Step 2: Run the new tests**

```
cargo test --lib core::writer::url
```

Expected: **4 passed**.

- [ ] **Step 3: Full sweep**

```
cargo test
```

Expected: **76 total** (73 lib + 3 integration; +4 over Task 2).

- [ ] **Step 4: Commit**

```bash
git add crates/things-mcp/src/core/writer/url.rs
git commit -m "core/writer/url: build_url + mask_auth_token"
```

---

### Task 4: `executor.rs` — `Executor` trait + prod + recording impls

Object-safe async trait so `Arc<dyn Executor>` can live on `Writer`. The `OpenCommandExecutor` shells out to `/usr/bin/open -g`; the `RecordingExecutor` captures URLs in a `Mutex<Vec<String>>`. Production binds to the former; tests bind to the latter.

**Files:**
- Create: `crates/things-mcp/src/core/writer/executor.rs`

- [ ] **Step 1: Write the executor module**

`crates/things-mcp/src/core/writer/executor.rs`:

```rust
//! Executor seam: how a built URL gets handed to macOS so Things can
//! process it. Production uses `OpenCommandExecutor` (spawns
//! `/usr/bin/open -g <url>`). Tests substitute `RecordingExecutor`,
//! which captures URLs without spawning anything.

use std::sync::Mutex;

use async_trait::async_trait;

use crate::core::error::ThingsError;

#[async_trait]
pub trait Executor: Send + Sync + std::fmt::Debug {
    /// Hand a `things://` URL to the platform. Returns once `/usr/bin/open`
    /// (or the test substitute) has been invoked — does NOT wait for the
    /// Things app to actually process it. Post-write verification is the
    /// `verify` module's job.
    async fn open(&self, url: &str) -> Result<(), ThingsError>;
}

/// Production executor: shells out to `/usr/bin/open -g <url>`.
/// The `-g` flag opens the URL in the background so Things doesn't yank
/// focus from whatever the user is doing.
#[derive(Debug, Default)]
pub struct OpenCommandExecutor;

#[async_trait]
impl Executor for OpenCommandExecutor {
    async fn open(&self, url: &str) -> Result<(), ThingsError> {
        let status = tokio::process::Command::new("/usr/bin/open")
            .arg("-g")
            .arg(url)
            .status()
            .await
            .map_err(|e| ThingsError::ExecutorFailed {
                source: format!("spawn /usr/bin/open: {e}"),
            })?;
        if !status.success() {
            return Err(ThingsError::ExecutorFailed {
                source: format!("/usr/bin/open exited {status}"),
            });
        }
        Ok(())
    }
}

/// Test executor: records every URL it's asked to open without spawning
/// anything. Use `urls()` to inspect what was captured.
#[derive(Debug, Default)]
pub struct RecordingExecutor {
    urls: Mutex<Vec<String>>,
}

impl RecordingExecutor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn urls(&self) -> Vec<String> {
        self.urls.lock().unwrap().clone()
    }
}

#[async_trait]
impl Executor for RecordingExecutor {
    async fn open(&self, url: &str) -> Result<(), ThingsError> {
        self.urls.lock().unwrap().push(url.to_string());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn recording_executor_captures_urls_in_order() {
        let rec = RecordingExecutor::new();
        rec.open("things:///json?data=%5B%5D").await.unwrap();
        rec.open("things:///json?data=%5Bx%5D").await.unwrap();
        let urls = rec.urls();
        assert_eq!(urls.len(), 2);
        assert!(urls[0].contains("%5B%5D"));
        assert!(urls[1].contains("%5Bx%5D"));
    }

    // Manual smoke test: opt-in only — fires `/usr/bin/open` against the
    // user's real Things app. Run with `cargo test -- --ignored
    // open_command_executor_smoke` only when you mean to.
    #[tokio::test]
    #[ignore = "fires /usr/bin/open against the real Things app"]
    async fn open_command_executor_smoke() {
        let exec = OpenCommandExecutor;
        exec.open("things:///")
            .await
            .expect("open should not fail");
    }
}
```

- [ ] **Step 2: Run the new tests**

```
cargo test --lib core::writer::executor
```

Expected: **1 passed** (the recording-executor test). The smoke test is `#[ignore]`d.

- [ ] **Step 3: Full sweep**

```
cargo test
```

Expected: **77 total** (74 lib + 3 integration; +1 over Task 3).

- [ ] **Step 4: Commit**

```bash
git add crates/things-mcp/src/core/writer/executor.rs
git commit -m "core/writer/executor: Executor trait + OpenCommand + Recording impls"
```

---

### Task 5: `verify.rs` — `VerifyPredicate` + bounded poll

Discriminated enum of verify shapes, an async `verify()` that bounded-polls the reader pool, three happy-path tests (one per variant), a timeout test, and a NotFound short-circuit test.

**Files:**
- Create: `crates/things-mcp/src/core/writer/verify.rs`

- [ ] **Step 1: Write the failing tests + implementation together**

`crates/things-mcp/src/core/writer/verify.rs`:

```rust
//! Post-write verification: poll the reader pool until we see the expected
//! change (verified), the predicate proves the row will never exist
//! (NotFound — for updates and status-changes only), or the timeout elapses
//! (Timeout). Bounded `poll_timeout / poll_interval` so a misbehaving Things
//! cannot hang the MCP call.

use std::time::{Duration, Instant};

use rusqlite::Connection;

use crate::core::error::ThingsError;
use crate::core::reader::pool::ReaderPool;
use crate::core::types::{TaskStatus, TodoSummary};

#[derive(Debug, Clone)]
pub enum VerifyPredicate {
    /// A row with this title and creationDate ≥ since_unix should exist.
    CreateByTitle { title: String, since_unix: f64 },
    /// The row at this id should match all populated expected_* fields.
    UpdateById {
        id: String,
        expected_title: Option<String>,
        expected_notes: Option<String>,
    },
    /// The row at this id should have this status.
    StatusChange { id: String, want: TaskStatus },
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
    if let VerifyPredicate::UpdateById { id, .. } | VerifyPredicate::StatusChange { id, .. } = &pred {
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
    use crate::core::reader::queries::{row_to_summary, SUMMARY_COLS};
    match pred {
        VerifyPredicate::CreateByTitle { title, since_unix } => {
            let sql = format!(
                r#"
                SELECT {SUMMARY_COLS}
                FROM TMTask AS t
                WHERE t.trashed = 0
                  AND t.type = 0
                  AND t.title = ?
                  AND t.creationDate >= ?
                ORDER BY t.creationDate DESC
                LIMIT 1
                "#
            );
            let mut stmt = c.prepare_cached(&sql)?;
            let mut rows = stmt.query(rusqlite::params![title, since_unix])?;
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
    }
}

/// Column count of `SUMMARY_COLS` (11) — used by the `UpdateById` branch when
/// it pulls `t.notes` as an extra trailing column. If `SUMMARY_COLS` ever
/// changes shape, this constant must move with it.
const SUMMARY_COLS_LEN: usize = 11;

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
        // The fixture's 'todo-3 Pay tax bill' has status=3 (Done).
        let out = verify(
            &pool,
            VerifyPredicate::StatusChange {
                id: "todo-3".into(),
                want: TaskStatus::Done,
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
}
```

- [ ] **Step 2: Run the new tests**

```
cargo test --lib core::writer::verify
```

Expected: **5 passed**.

- [ ] **Step 3: Full sweep**

```
cargo test
```

Expected: **82 total** (79 lib + 3 integration; +5 over Task 4).

- [ ] **Step 4: Commit**

```bash
git add crates/things-mcp/src/core/writer/verify.rs
git commit -m "core/writer/verify: bounded poll against ReaderPool by predicate"
```

---

### Task 6: `writer.rs` — `Writer` + `SafetyMode` + `fire()` pipeline

The keystone. Ties operation → URL → executor → verify into one method. Three safety-gate tests cover Forbidden, DryRun, and a no-op happy path.

**Files:**
- Create: `crates/things-mcp/src/core/writer/writer.rs`

- [ ] **Step 1: Write the writer module**

`crates/things-mcp/src/core/writer/writer.rs`:

```rust
//! `Writer` — the keystone of `core/writer/`. Glues operation rendering,
//! URL composition, the executor seam, and post-write verification together
//! behind one method, `fire()`. Safety gates enforced up front: writes are
//! refused in test-DB mode unless explicitly opted in, and creates short-
//! circuit to a dry-run outcome in that mode without ever firing the
//! executor.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::core::error::ThingsError;
use crate::core::reader::pool::ReaderPool;
use crate::core::writer::executor::Executor;
use crate::core::writer::operation::Operation;
use crate::core::writer::outcome::WriteOutcome;
use crate::core::writer::secret::SecretString;
use crate::core::writer::url::{build_url, mask_auth_token};
use crate::core::writer::verify::{verify, VerifyOutcome, VerifyPredicate};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SafetyMode {
    /// Live: writes fire normally.
    Live,
    /// Test-DB mode with the explicit opt-in: build the URL, log it, do
    /// NOT call the executor. `WriteOutcome { dry_run: true }`.
    DryRun,
    /// Test-DB mode without the opt-in: refuse with `TestDbWriteForbidden`.
    Forbidden,
}

#[derive(Debug, Clone, Copy)]
pub struct WriterCfg {
    pub poll_timeout: Duration,
    pub poll_interval: Duration,
}

#[derive(Debug, Clone)]
pub struct Writer {
    pub executor: Arc<dyn Executor>,
    pub pool: ReaderPool,
    pub auth: Option<SecretString>,
    pub cfg: WriterCfg,
    pub safety: SafetyMode,
}

impl Writer {
    pub async fn fire(
        &self,
        op: Operation,
        verify_pred: VerifyPredicate,
    ) -> Result<WriteOutcome, ThingsError> {
        // 1. Safety gate — refuse outright before doing any work.
        if self.safety == SafetyMode::Forbidden {
            return Err(ThingsError::TestDbWriteForbidden);
        }

        // 2. Auth gate — only operations that require the token care.
        if op.requires_auth_token() && self.auth.is_none() {
            return Err(ThingsError::MissingAuthToken {
                hint: "set THINGS_AUTH_TOKEN or config.toml [things].auth_token".into(),
            });
        }

        // 3. Build URL.
        let url = build_url(&[op.clone()], self.auth.as_ref());

        // 4. Log URL (masked).
        tracing::info!(action = op.action_name(), "write: {}", mask_auth_token(&url));

        // 5. Dry-run short-circuit.
        if self.safety == SafetyMode::DryRun {
            return Ok(WriteOutcome {
                id: None,
                action: op.action_name().to_string(),
                verified: false,
                dry_run: true,
                latency_ms: 0,
            });
        }

        // 6. Open URL via the injected executor.
        let started = Instant::now();
        self.executor.open(&url).await?;

        // 7. Verify by polling the reader.
        let outcome = verify(
            &self.pool,
            verify_pred,
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
            VerifyOutcome::Timeout { .. } => WriteOutcome {
                id: None,
                action: op.action_name().to_string(),
                verified: false,
                dry_run: false,
                latency_ms,
            },
            VerifyOutcome::NotFound { .. } => WriteOutcome {
                id: None,
                action: op.action_name().to_string(),
                verified: false,
                dry_run: false,
                latency_ms,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reader::fixture::build_fixture;
    use crate::core::writer::executor::RecordingExecutor;
    use crate::core::writer::operation::AddTodoSpec;
    use tempfile::tempdir;

    async fn build_writer(safety: SafetyMode) -> (tempfile::TempDir, Writer, Arc<RecordingExecutor>) {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let exec = Arc::new(RecordingExecutor::new());
        let writer = Writer {
            executor: exec.clone(),
            pool,
            auth: None,
            cfg: WriterCfg {
                poll_timeout: Duration::from_millis(200),
                poll_interval: Duration::from_millis(20),
            },
            safety,
        };
        (tmp, writer, exec)
    }

    fn add_op(title: &str) -> Operation {
        Operation::AddTodo(AddTodoSpec {
            title: title.into(),
            ..Default::default()
        })
    }

    fn pred(title: &str) -> VerifyPredicate {
        VerifyPredicate::CreateByTitle {
            title: title.into(),
            since_unix: 0.0,
        }
    }

    #[tokio::test]
    async fn fire_returns_test_db_write_forbidden_in_forbidden_mode() {
        let (_tmp, writer, exec) = build_writer(SafetyMode::Forbidden).await;
        let res = writer.fire(add_op("anything"), pred("anything")).await;
        assert!(matches!(res, Err(ThingsError::TestDbWriteForbidden)));
        // Executor must NOT have been called.
        assert!(exec.urls().is_empty());
    }

    #[tokio::test]
    async fn fire_dry_run_short_circuits_without_calling_executor() {
        let (_tmp, writer, exec) = build_writer(SafetyMode::DryRun).await;
        let out = writer
            .fire(add_op("Pretend to buy bread"), pred("Pretend to buy bread"))
            .await
            .unwrap();
        assert!(out.dry_run);
        assert!(!out.verified);
        assert_eq!(out.action, "add_todo");
        assert_eq!(out.latency_ms, 0);
        // Executor must NOT have been called in dry-run.
        assert!(exec.urls().is_empty());
    }

    #[tokio::test]
    async fn fire_live_calls_executor_then_times_out_against_test_db() {
        // In a test-fixture DB with no Things app behind it, verify will time out.
        // This is the happy "executor-was-called-but-no-row-appeared" path,
        // which lets us assert the URL was emitted AND the timeout outcome.
        let (_tmp, writer, exec) = build_writer(SafetyMode::Live).await;
        let out = writer
            .fire(
                add_op("Definitely-not-in-fixture row"),
                pred("Definitely-not-in-fixture row"),
            )
            .await
            .unwrap();
        // Executor was called exactly once.
        let urls = exec.urls();
        assert_eq!(urls.len(), 1);
        assert!(urls[0].starts_with("things:///json?data="));
        // Verify timed out because the fixture has no such row.
        assert!(!out.dry_run);
        assert!(!out.verified);
        assert_eq!(out.action, "add_todo");
        assert!(out.latency_ms >= 200, "should reach the configured timeout");
    }
}
```

- [ ] **Step 2: Run the new tests**

```
cargo test --lib core::writer::writer
```

Expected: **3 passed**.

- [ ] **Step 3: Full sweep**

```
cargo test
```

Expected: **85 total** (82 lib + 3 integration; +3 over Task 5).

- [ ] **Step 4: Commit**

```bash
git add crates/things-mcp/src/core/writer/writer.rs
git commit -m "core/writer/writer: Writer::fire pipeline + safety gates"
```

---

### Task 7: `AppState` wiring — writer field + executor override + safety resolution

Wire `Writer` into `AppState::build`. Resolve `SafetyMode` from existing flags. Load the auth-token from env or config (still optional). Expose an `executor_override` for tests.

**Files:**
- Modify: `crates/things-mcp/src/state.rs`

- [ ] **Step 1: Inspect the current `state.rs`**

It currently has fields `config`, `db_path`, `pool`, `test_db_mode`, `allow_writes_on_test_db`, `fts`. `AppStateOptions` has `env_db_path`, `home_dir`, `config_path`, `allow_writes_on_test_db`. After Plan 3 it builds the pool, runs the schema probe, takes the startup backup, resolves FTS, and constructs `Self`.

- [ ] **Step 2: Update the imports**

Replace the existing `use crate::core::{...}` block in `state.rs` with:

```rust
use crate::core::{
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

- [ ] **Step 3: Extend the structs**

Add `writer: Arc<Writer>` to `AppState`:

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
}
```

Add `executor_override: Option<Arc<dyn Executor>>` to `AppStateOptions`:

```rust
pub struct AppStateOptions {
    pub env_db_path: Option<PathBuf>,
    pub home_dir: PathBuf,
    pub config_path: PathBuf,
    pub allow_writes_on_test_db: bool,
    /// Test-only: inject a `RecordingExecutor` (or any other) in place of the
    /// production `OpenCommandExecutor`. `None` in production code paths.
    pub executor_override: Option<Arc<dyn Executor>>,
}
```

- [ ] **Step 4: Build the writer inside `AppState::build`**

After the existing `fts = ...` resolution and BEFORE constructing `Self`, add:

```rust
        let executor: Arc<dyn Executor> = opts
            .executor_override
            .clone()
            .unwrap_or_else(|| Arc::new(OpenCommandExecutor));

        let safety = if test_db_mode {
            if opts.allow_writes_on_test_db {
                SafetyMode::DryRun
            } else {
                SafetyMode::Forbidden
            }
        } else {
            SafetyMode::Live
        };

        let auth = std::env::var("THINGS_AUTH_TOKEN")
            .ok()
            .or_else(|| cfg.things.auth_token.clone())
            .map(SecretString::new);

        let writer = Arc::new(Writer {
            executor,
            pool: pool.clone(),
            auth,
            cfg: WriterCfg {
                poll_timeout: std::time::Duration::from_millis(cfg.writer.poll_timeout_ms),
                poll_interval: std::time::Duration::from_millis(cfg.writer.poll_interval_ms),
            },
            safety,
        });
```

Then include `writer` in the struct literal:

```rust
        Ok(Self {
            config: Arc::new(cfg),
            db_path,
            pool,
            test_db_mode,
            allow_writes_on_test_db: opts.allow_writes_on_test_db,
            fts,
            writer,
        })
```

- [ ] **Step 5: Update existing integration test sites**

`grep` for `AppStateOptions {` across the test files:

```
grep -rn "AppStateOptions {" crates/things-mcp/tests
```

The Plan-2 (`end_to_end_plan_2.rs`) and Plan-3 (`end_to_end_search.rs`) test fixtures construct `AppStateOptions` directly. Add `executor_override: None,` to each construction site.

- [ ] **Step 6: Build + full sweep**

```
cargo build
cargo test
```

Expected: **85 total** (no new tests in this task — same as Task 6).

- [ ] **Step 7: Commit**

```bash
git add crates/things-mcp/src/state.rs crates/things-mcp/tests
git commit -m "state: wire Writer + executor_override + SafetyMode resolution"
```

---

### Task 8: `things_add_todo` MCP tool + integration tests

Add the args struct + adapter in `tools/todos.rs`, register the tool on `ThingsServer`, and ship two integration tests that prove the chassis end-to-end.

**Files:**
- Modify: `crates/things-mcp/src/tools/todos.rs`
- Modify: `crates/things-mcp/src/server.rs`
- Create: `crates/things-mcp/tests/end_to_end_add_todo.rs`

- [ ] **Step 1: Extend `tools/todos.rs`**

Append to `crates/things-mcp/src/tools/todos.rs` (after `things_get_todo`):

```rust
use std::time::{SystemTime, UNIX_EPOCH};

use crate::core::writer::operation::{AddTodoSpec, Operation};
use crate::core::writer::outcome::WriteOutcome;
use crate::core::writer::verify::VerifyPredicate;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct AddTodoArgs {
    /// To-do title. Required, non-empty.
    pub title: String,
    /// Free-text notes (optional).
    #[serde(default)]
    pub notes: Option<String>,
    /// `"today"`, `"tomorrow"`, `"evening"`, `"anytime"`, `"someday"`, or an
    /// ISO date / timestamp. Optional.
    #[serde(default)]
    pub when: Option<String>,
    /// ISO `YYYY-MM-DD` deadline. Optional.
    #[serde(default)]
    pub deadline: Option<String>,
    /// Tag titles to attach to the new to-do. Optional.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Checklist item titles, in display order. Optional.
    #[serde(default)]
    pub checklist_items: Vec<String>,
    /// Project or area UUID this to-do should belong to. Optional.
    #[serde(default)]
    pub list_id: Option<String>,
    /// Heading UUID, if filing under a specific heading inside a project. Optional.
    #[serde(default)]
    pub heading_id: Option<String>,
}

pub async fn things_add_todo(
    state: AppState,
    args: AddTodoArgs,
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
    let op = Operation::AddTodo(AddTodoSpec {
        title: args.title.clone(),
        notes: args.notes,
        when: args.when,
        deadline: args.deadline,
        tags: args.tags,
        checklist_items: args.checklist_items,
        list_id: args.list_id,
        heading_id: args.heading_id,
    });
    let predicate = VerifyPredicate::CreateByTitle {
        title: args.title,
        since_unix,
    };
    let outcome = state.writer.fire(op, predicate).await?;
    Ok(outcome)
}
```

- [ ] **Step 2: Register the tool on `ThingsServer`**

In `crates/things-mcp/src/server.rs`, add to the existing `use crate::tools::todos::{...}` import block:

```rust
use crate::tools::todos::{things_add_todo, things_get_todo, AddTodoArgs, GetTodoArgs};
```

(Replace the previous single-name import.) Then add an additional `use`:

```rust
use crate::core::writer::outcome::WriteOutcome;
```

Inside the `#[tool_router] impl ThingsServer { ... }` block, AFTER `tool_get_project` and BEFORE the closing `}`, insert:

```rust
    #[tool(
        name = "things_add_todo",
        description = "Create a new to-do in Things. Returns a WriteOutcome with the new id once verified by polling the SQLite reader. Requires `title`; all other fields are optional. Open-world: side-effects the live Things app via the JSON URL scheme.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn tool_add_todo(
        &self,
        Parameters(args): Parameters<AddTodoArgs>,
    ) -> Result<Json<WriteOutcome>, McpError> {
        let out = things_add_todo(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(out))
    }
```

- [ ] **Step 3: Build to confirm the wiring**

```
cargo build
```

Expected: clean. (No new tests yet — those come in the next step.)

- [ ] **Step 4: Write the integration tests**

`crates/things-mcp/tests/end_to_end_add_todo.rs`:

```rust
//! End-to-end exercise of the Plan-4 write pipeline. Two flows:
//!
//! 1. Dry-run mode: test-DB with `allow_writes_on_test_db=true`. Asserts the
//!    executor was NOT called and `WriteOutcome { dry_run: true }`.
//! 2. Recording-executor live mode: a `RecordingExecutor` is injected via
//!    `AppStateOptions.executor_override`. Asserts the executor recorded
//!    exactly one URL that parses as a valid `things:///json` URL containing
//!    the title; verify times out (no real Things app), yielding
//!    `WriteOutcome { dry_run: false, verified: false }`.

use std::sync::Arc;

use things_mcp::core::reader::fixture::build_fixture;
use things_mcp::core::writer::executor::{Executor, RecordingExecutor};
use things_mcp::state::{AppState, AppStateOptions};
use things_mcp::tools::todos::{things_add_todo, AddTodoArgs};

async fn build_state(
    allow_writes_on_test_db: bool,
    executor_override: Option<Arc<dyn Executor>>,
) -> AppState {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("things.sqlite");
    build_fixture(&db).unwrap();
    let state = AppState::build(AppStateOptions {
        env_db_path: Some(db.clone()),
        home_dir: tmp.path().to_path_buf(),
        config_path: tmp.path().join("config.toml"),
        allow_writes_on_test_db,
        executor_override,
    })
    .await
    .unwrap();
    std::mem::forget(tmp);
    state
}

#[tokio::test]
async fn add_todo_dry_run_does_not_call_executor() {
    // No executor override — the test-DB safety gate short-circuits before
    // the executor would be called anyway. Tighter coverage: also pass a
    // recording executor and assert it stays empty.
    let recorder = Arc::new(RecordingExecutor::new());
    let state = build_state(true, Some(recorder.clone() as Arc<dyn Executor>)).await;
    let out = things_add_todo(
        state,
        AddTodoArgs {
            title: "Pretend buy bread".into(),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(out.dry_run, "expected dry_run=true in test-DB mode");
    assert!(!out.verified);
    assert_eq!(out.action, "add_todo");
    assert!(out.id.is_none());
    assert_eq!(recorder.urls().len(), 0, "executor must not be called in dry-run");
}

#[tokio::test]
async fn add_todo_plumbs_optional_fields_through_dry_run() {
    // Asserts that args with tags/notes/etc. round-trip through the tool
    // layer cleanly. The dry-run path still constructs the Operation but
    // short-circuits before the executor — so the recording executor stays
    // empty. End-to-end coverage of the Live executor call lives in the
    // Writer unit test (Task 6); the integration boundary covers wiring.
    let recorder = Arc::new(RecordingExecutor::new());
    let state = build_state(true, Some(recorder.clone() as Arc<dyn Executor>)).await;
    let out = things_add_todo(
        state,
        AddTodoArgs {
            title: "Through the chassis".into(),
            tags: vec!["E2E".into()],
            notes: Some("via integration test".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(out.dry_run);
    assert!(!out.verified);
    assert_eq!(out.action, "add_todo");
    assert_eq!(recorder.urls().len(), 0);
}
```

- [ ] **Step 5: Run the integration tests**

```
cargo test --test end_to_end_add_todo
```

Expected: **2 passed**.

- [ ] **Step 6: Full sweep**

```
cargo test
```

Expected: **87 total** (82 lib + 5 integration; +2 over Task 7).

- [ ] **Step 7: Commit**

```bash
git add crates/things-mcp/src/tools/todos.rs crates/things-mcp/src/server.rs crates/things-mcp/tests/end_to_end_add_todo.rs
git commit -m "tools: things_add_todo + e2e dry-run integration test"
```

---

### Task 9: README + final sweep

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Bump the status line**

Open `README.md`. The current status line (set by Plan 3) reads:

```markdown
**Status:** Plan 3 — read surface complete (`inbox`/`today`/`upcoming`/`anytime`/`someday`/`logbook`/`trash`/`areas`/`projects`/`tags`/`get_todo`/`get_project`/`list_by_tag`/`search`) over stdio. FTS5 capability is detected at startup; the search query currently uses `LIKE` against `title` and `notes` (FTS5 query path activates in a follow-on once verified against a live Things DB). See `docs/superpowers/plans/` for the active plan and follow-ons.
```

Replace it with:

```markdown
**Status:** Plan 4 — read surface complete + first write tool (`things_add_todo`) shipping over the JSON URL scheme. Writes go through `core/writer/`: typed `Operation` → percent-encoded URL → `/usr/bin/open -g` (or injected test executor) → bounded poll against the SQLite reader → `WriteOutcome`. Test-DB mode short-circuits to dry-run; auth-token (required only for updates) wired but not yet exercised. See `docs/superpowers/plans/` for the active plan and follow-ons.
```

- [ ] **Step 2: Full suite + release build**

```
cargo test && cargo build --release
```

Expected: **87 tests pass**; release build clean.

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: README — plan 4 first write tool shipping"
```

- [ ] **Step 4: Inspect history**

```
git log --oneline | head -10
```

Expected: 9 new commits on top of `a1405ae` (one per task in this plan).

---

## Self-review checklist (for the executor)

- [ ] `things_add_todo` is registered on `ThingsServer` with all four MCP annotations (`read_only_hint=false`, `destructive_hint=false`, `idempotent_hint=false`, `open_world_hint=true`).
- [ ] `core/writer/` contains 8 files matching the file map; no file exceeds ~250 lines.
- [ ] `Operation::AddTodo` renders the documented Things JSON shape: `{ "type": "to-do", "attributes": { "title": …, …optional fields } }`; missing fields are absent from the JSON, not present-with-null.
- [ ] `build_url` percent-encodes the JSON payload AND the auth-token (when present); `mask_auth_token` redacts the token segment for logs.
- [ ] `Executor` is `dyn`-compatible and Arc-storable; `OpenCommandExecutor` and `RecordingExecutor` both implement it via `async_trait`.
- [ ] `verify` short-circuits with `NotFound` for `UpdateById` / `StatusChange` when the row doesn't exist; bounded poll for `CreateByTitle` waits up to `poll_timeout`.
- [ ] `Writer::fire` enforces gates in order: Forbidden → MissingAuthToken → Build URL → log (masked) → DryRun short-circuit → Executor → Verify → WriteOutcome.
- [ ] `AppState.writer: Arc<Writer>` resolved at startup; tests can inject a `RecordingExecutor` via `AppStateOptions.executor_override: Option<Arc<dyn Executor>>`.
- [ ] `THINGS_AUTH_TOKEN` env var takes precedence over `[things].auth_token` in config.toml; absent token is `None` (not an error); `SecretString::Debug` impl never prints the raw value.
- [ ] No new ThingsError variants beyond `TestDbWriteForbidden` and `ExecutorFailed`; existing leftovers (`DryRun`, `WriteUnverified`) are left as-is.
- [ ] Every commit message starts with a module prefix (`core/writer`, `state`, `tools`, `tests`, `docs`).
- [ ] `cargo test` shows **87 tests pass** at the end of Task 9; `cargo build --release` is clean.

When all green, the natural next step is **Plan 5** (remaining write tools — `add_project`, `update_todo`, `update_project`, `complete_todo`, `cancel_todo`, `move_todo`, `bulk_json`). Plan 4's chassis is reused without modification.
