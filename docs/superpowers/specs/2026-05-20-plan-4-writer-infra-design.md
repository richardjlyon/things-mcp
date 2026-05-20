# Plan 4 — writer infrastructure + `things_add_todo`

**Status:** design draft, 2026-05-20

**Goal.** Ship `core/writer/`: the chassis that turns a typed write operation into a Things JSON URL, fires it via `/usr/bin/open -g`, polls the SQLite reader for the expected change, and returns a `WriteOutcome`. Prove it end-to-end with a single MCP write tool, `things_add_todo`.

**Why this scope.** Plans 1–3 shipped the read pipeline. The first write tool surfaces every load-bearing concern at once: payload construction, URL encoding, executor seam, verification, safety gating, and the wiring into `AppState`. Solving it once with one tool lets Plan 5 layer the remaining ~7 write tools on top without re-litigating the chassis.

**Predecessor.** `docs/superpowers/plans/2026-05-20-plan-3-search.md` (committed at `8bcb6e0`). HEAD test count: 65.

**Parent design.** `docs/superpowers/specs/2026-05-20-things-mcp-server-design.md` §3 (crate layout, `core/writer/`) and §5 (write path pipeline, dry-run, backups, auth-token discipline).

**Reference repo.** `zotero-connector` — same dual-transport / launchd / OAuth pattern landing in Plans 8 + 9. No analog for the JSON-URL writer; that's specific to this server.

---

## 1. Architecture

```
crates/things-mcp/src/core/writer/
├── mod.rs          module root + re-exports
├── operation.rs    Operation enum + render_json()
├── url.rs          build_url(&[Operation], Option<&SecretString>) -> String
├── executor.rs     trait Executor + OpenCommandExecutor + RecordingExecutor (test)
├── verify.rs       VerifyPredicate enum + verify(pool, pred, timeout) -> VerifyOutcome
├── outcome.rs      WriteOutcome { id, action, verified, dry_run, latency_ms }
└── writer.rs       Writer { executor, pool, auth, cfg } + Writer::fire(op, verify)

crates/things-mcp/src/tools/
└── todos.rs        adds things_add_todo (AddTodoArgs → Writer::fire(AddTodo, verify))

crates/things-mcp/src/state.rs
   AppState gains `writer: Arc<Writer>`
   AppStateOptions gains `executor_override: Option<Arc<dyn Executor>>`

crates/things-mcp/src/core/config.rs
   Config gains `[writer]` section (poll_timeout_ms, poll_interval_ms)
```

Each module has one clear responsibility and a narrow public interface, mirroring `core/reader/`'s file split. No file should grow beyond ~250 lines; if it does, that's a signal to split before merging.

## 2. Operation model

```rust
pub enum Operation {
    AddTodo(AddTodoSpec),
    // Plan 5: AddProject, UpdateTodo, UpdateProject, etc.
}

pub struct AddTodoSpec {
    pub title: String,
    pub notes: Option<String>,
    pub when: Option<String>,         // "today", "tomorrow", "evening", "anytime",
                                      // "someday", ISO date, or ISO timestamp
    pub deadline: Option<String>,     // ISO YYYY-MM-DD
    pub tags: Vec<String>,            // tag titles
    pub checklist_items: Vec<String>,
    pub list_id: Option<String>,      // project or area UUID
    pub heading_id: Option<String>,
}

impl Operation {
    pub fn render_json(&self) -> serde_json::Value { /* one-element array element */ }
    pub fn action_name(&self) -> &'static str { /* "add_todo", etc. */ }
    pub fn requires_auth_token(&self) -> bool { /* false for creates, true for updates */ }
}
```

`render_json` emits one operation object in the shape Things expects (per [Things URL JSON spec](https://culturedcode.com/things/support/articles/2803573)):

```json
{
  "type": "to-do",
  "attributes": {
    "title": "Buy milk",
    "notes": "From the corner shop",
    "when": "today",
    "deadline": "2026-05-22",
    "tags": ["Errand"],
    "checklist-items": [
      { "type": "checklist-item", "attributes": { "title": "Bread" } }
    ],
    "list-id": "abc-123",
    "heading": "head-1"
  }
}
```

The full URL payload wraps one or more such objects in a JSON array: `[ { ... } ]`. `Writer::fire` accepts a single `Operation` in Plan 4; Plan 5 may extend to `&[Operation]` for batches.

## 3. URL construction (`url.rs`)

```rust
pub fn build_url(
    ops: &[Operation],
    auth_token: Option<&SecretString>,
) -> String
```

Behavior:

1. Render each operation to a JSON `Value`, collect into a JSON array.
2. Minify with `serde_json::to_string` (no pretty-printing — wastes bytes in the URL).
3. Percent-encode using `percent_encoding::utf8_percent_encode` with the `NON_ALPHANUMERIC` set.
4. Compose: `things:///json?data=<encoded>` + `&auth-token=<encoded>` if `auth_token` is Some.

The auth-token is read out of the `SecretString` via `expose_secret()` exactly once, percent-encoded, and dropped. The full URL string is treated as secret-bearing — never logged in full when an auth-token is present; loggers must mask the `auth-token=…` segment.

For Plan 4 (AddTodo creates only), `auth_token` is `None` and no masking is needed. But the masking helper is shipped now so Plan 5 inherits a tested implementation.

## 4. Executor (`executor.rs`)

```rust
#[async_trait::async_trait]
pub trait Executor: Send + Sync + std::fmt::Debug {
    async fn open(&self, url: &str) -> Result<(), ExecutorError>;
}

#[derive(Debug, Default)]
pub struct OpenCommandExecutor;

#[async_trait::async_trait]
impl Executor for OpenCommandExecutor {
    async fn open(&self, url: &str) -> Result<(), ExecutorError> {
        // tokio::process::Command::new("/usr/bin/open").arg("-g").arg(url)
        //   .spawn() then await with timeout.
        // -g = background (do not bring Things to front).
    }
}

#[derive(Debug, Default)]
pub struct RecordingExecutor {
    urls: Mutex<Vec<String>>,
}

impl RecordingExecutor {
    pub fn urls(&self) -> Vec<String> { /* clone of recorded list */ }
}
```

Use `async_trait::async_trait` to keep the trait object-safe and avoid GATs gymnastics. `async_trait` is already in the wider Rust MCP/rmcp ecosystem; if not already in `Cargo.toml`, Plan 4 adds it as a direct dep.

The `Executor` trait is dyn-safe so `Arc<dyn Executor>` can be stored on the writer.

## 5. Verification (`verify.rs`)

```rust
pub enum VerifyPredicate {
    CreateByTitle { title: String, since_unix: f64 },
    UpdateById   { id: String, expected_title: Option<String>, expected_notes: Option<String> },
    StatusChange { id: String, want: TaskStatus },
}

pub enum VerifyOutcome {
    Verified { row: TodoSummary, latency_ms: u64 },
    Timeout  { latency_ms: u64 },
    NotFound { latency_ms: u64 },  // for UpdateById/StatusChange where the row never existed
}

pub async fn verify(
    pool: &ReaderPool,
    pred: VerifyPredicate,
    timeout: Duration,
    interval: Duration,
) -> Result<VerifyOutcome, ThingsError>
```

Bounded poll loop:

```rust
let start = Instant::now();
loop {
    let check_result = check(pool, &pred).await?;
    if let Some(found) = check_result {
        return Ok(VerifyOutcome::Verified { row: found, latency_ms: start.elapsed_ms() });
    }
    if start.elapsed() >= timeout {
        return Ok(VerifyOutcome::Timeout { latency_ms: start.elapsed_ms() });
    }
    tokio::time::sleep(interval).await;
}
```

`check()` is a private helper that dispatches on the predicate and runs one SELECT against the reader pool. Each variant maps cleanly to existing query patterns:

- `CreateByTitle`: `SELECT * FROM TMTask WHERE title = ? AND creationDate >= ? AND trashed = 0 AND type = 0 ORDER BY creationDate DESC LIMIT 1`
- `UpdateById`: `SELECT * FROM TMTask WHERE uuid = ? AND trashed = 0` — then compare against expected_* fields; only succeed if all expected fields match.
- `StatusChange`: `SELECT status FROM TMTask WHERE uuid = ?` — succeed when status matches `want`.

For `UpdateById` and `StatusChange`, if the row never existed, we don't want to poll for 3 seconds. A short pre-check (`SELECT EXISTS`) returns `NotFound` immediately. Plan 4 ships this short-circuit only for these two variants (not for CreateByTitle, which expects the row to NOT exist initially).

**Defaults from config:** `poll_timeout_ms=3000`, `poll_interval_ms=100`. Bounded so a misbehaving Things state cannot hang the MCP call.

## 6. Outcome (`outcome.rs`)

```rust
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct WriteOutcome {
    /// The id of the affected row. For CreateByTitle, populated from the verified row.
    /// For UpdateById/StatusChange, echoes the input id. None when verify timed out.
    pub id: Option<String>,
    /// "add_todo", "update_todo", etc. — Operation::action_name().
    pub action: String,
    /// True iff verify() returned VerifyOutcome::Verified.
    pub verified: bool,
    /// True when the writer short-circuited at the dry-run gate.
    pub dry_run: bool,
    /// Total latency (build + open + verify), milliseconds.
    pub latency_ms: u64,
}
```

`WriteOutcome` is returned by every write tool. `verified=true` + `dry_run=false` is the happy path; the other combinations encode the various failure / safe modes.

## 7. The `Writer` (`writer.rs`)

Glues 2–6 together:

```rust
pub struct Writer {
    pub executor: Arc<dyn Executor>,
    pub pool: ReaderPool,
    pub auth: Option<SecretString>,
    pub cfg: WriterCfg,
    pub safety: SafetyMode,
}

pub struct WriterCfg {
    pub poll_timeout: Duration,
    pub poll_interval: Duration,
}

pub enum SafetyMode {
    Live,                // production: writes fire normally
    DryRun,              // test-DB + allow_writes_on_test_db: build URL, log it, skip executor
    Forbidden,           // test-DB + !allow_writes_on_test_db: refuse with TestDbWriteForbidden
}

impl Writer {
    pub async fn fire(
        &self,
        op: Operation,
        verify_pred: VerifyPredicate,
    ) -> Result<WriteOutcome, ThingsError>
}
```

`fire()` performs, in order:

1. **Safety gate.** Match `self.safety`:
   - `Forbidden` → return `ThingsError::TestDbWriteForbidden`.
   - other → proceed.
2. **Auth check.** If `op.requires_auth_token()` and `self.auth.is_none()` → return `ThingsError::MissingAuthToken`.
3. **Build URL.** `build_url(&[op], self.auth.as_ref())`.
4. **Log URL.** At `info` level, with auth-token masked. (Below INFO it's noise; above it's expected operator-visible signal.)
5. **Dry-run short-circuit.** If `SafetyMode::DryRun`, return `WriteOutcome { id: None, action: op.action_name(), verified: false, dry_run: true, latency_ms: 0 }`.
6. **Open URL.** `self.executor.open(&url).await?`. Wall-clock starts here for `latency_ms`.
7. **Verify.** `verify(&self.pool, verify_pred, self.cfg.poll_timeout, self.cfg.poll_interval).await?`.
8. **Compose outcome.** Map `VerifyOutcome` → `WriteOutcome`.

The `Writer` is constructed once in `AppState::build` and stored as `Arc<Writer>`. Tool functions clone the Arc and call `fire`.

## 8. AppState wiring

```rust
pub struct AppState {
    /* existing fields */
    pub writer: Arc<Writer>,
}

pub struct AppStateOptions {
    /* existing fields */
    pub executor_override: Option<Arc<dyn Executor>>,
}
```

In `AppState::build`:

```rust
let executor: Arc<dyn Executor> = opts
    .executor_override
    .unwrap_or_else(|| Arc::new(OpenCommandExecutor));

let safety = if test_db_mode {
    if opts.allow_writes_on_test_db { SafetyMode::DryRun } else { SafetyMode::Forbidden }
} else {
    SafetyMode::Live
};

let writer = Arc::new(Writer {
    executor,
    pool: pool.clone(),
    auth: load_auth_token(&cfg)?,  // None if absent — fine for AddTodo
    cfg: WriterCfg::from_config(&cfg.writer),
    safety,
});
```

`load_auth_token` looks at `THINGS_AUTH_TOKEN` env var first, then `[things].auth_token` in `config.toml`, returns `Option<SecretString>`. Missing token is not an error (creates work without one); update tools enforce its presence at `fire()` time via `requires_auth_token`.

## 9. Config

Extend `Config` with a `[writer]` section:

```toml
[writer]
poll_timeout_ms = 3000
poll_interval_ms = 100
```

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WriterConfig {
    #[serde(default = "default_poll_timeout_ms")]
    pub poll_timeout_ms: u64,
    #[serde(default = "default_poll_interval_ms")]
    pub poll_interval_ms: u64,
}
fn default_poll_timeout_ms() -> u64 { 3000 }
fn default_poll_interval_ms() -> u64 { 100 }
```

The existing Config integration tests load + round-trip the file; extend them to cover the new section.

## 10. The first write tool: `things_add_todo`

`crates/things-mcp/src/tools/todos.rs` (existing file from Plan 2) gains:

```rust
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct AddTodoArgs {
    pub title: String,
    #[serde(default)] pub notes: Option<String>,
    #[serde(default)] pub when: Option<String>,
    #[serde(default)] pub deadline: Option<String>,
    #[serde(default)] pub tags: Vec<String>,
    #[serde(default)] pub checklist_items: Vec<String>,
    #[serde(default)] pub list_id: Option<String>,
    #[serde(default)] pub heading_id: Option<String>,
}

pub async fn things_add_todo(
    state: AppState,
    args: AddTodoArgs,
) -> anyhow::Result<WriteOutcome>
```

And in `server.rs`:

```rust
#[tool(
    name = "things_add_todo",
    description = "Create a new to-do in Things. Returns a WriteOutcome with the new id once verified.",
    annotations(
        read_only_hint = false,
        destructive_hint = false,
        idempotent_hint = false,
        open_world_hint = true       // Things app is "the outside world"
    )
)]
async fn tool_add_todo(
    &self,
    Parameters(args): Parameters<AddTodoArgs>,
) -> Result<Json<WriteOutcome>, McpError> { /* ... */ }
```

`things_add_todo` constructs an `Operation::AddTodo`, captures `since_unix = SystemTime::now()` **before** calling fire (so any newly-created row's `creationDate` is ≥ this timestamp), builds `VerifyPredicate::CreateByTitle { title, since_unix }`, calls `state.writer.fire(...)`, and returns the outcome. Pre-condition validation (title nonempty, ISO dates parseable) happens at the tool layer with `ThingsError::InvalidInput` — same pattern as Plan-2 list tools.

## 11. Testing strategy

| Layer | Test | What it proves |
|---|---|---|
| Operation::render_json | unit (per shape) | JSON matches Things' documented format byte-for-byte for a representative input |
| build_url | unit | Percent-encoding round-trips; auth-token segment present iff provided; auth-token masking helper redacts correctly |
| OpenCommandExecutor | unit (ignored by default) | `#[ignore]` test that fires a no-op URL — opt-in for human verification only |
| RecordingExecutor | unit | Captures URLs in order; `urls()` returns them |
| verify::verify | unit (3, one per predicate) | Seed fixture row, call verify, assert Verified outcome with correct row |
| verify timeout | unit | Seed nothing, assert Timeout outcome at ~poll_timeout_ms |
| verify NotFound | unit | UpdateById against missing id, assert NotFound |
| Writer::fire safety gates | unit (3) | Forbidden, MissingAuthToken, DryRun each return the expected outcome |
| things_add_todo (dry-run) | integration | `test_db_mode + allow_writes`. Asserts the executor was NOT called and `WriteOutcome { dry_run: true, verified: false }` |
| things_add_todo (recording exec) | integration | Recording executor + test-db; assert URL recorded; verify times out → `WriteOutcome { dry_run: false, verified: false }` |

**No test fires `/usr/bin/open` against the user's live Things app.** The OpenCommandExecutor unit test is marked `#[ignore]` and only run manually during Plan 10 (manual E2E runbook).

Target test count: **65 → ~85** (≈ +20 tests across the writer modules and integration).

## 12. Errors

New variants on `ThingsError`:

```rust
TestDbWriteForbidden,                    // safety gate
MissingAuthToken { action: String },     // auth gate
WriteTimedOut { action: String, latency_ms: u64 },  // verify timeout (non-fatal — returned in outcome)
ExecutorFailed { source: ExecutorError },
```

`WriteTimedOut` is informational only — `fire()` returns `Ok(WriteOutcome { verified: false, ... })` rather than `Err`. The tool layer never converts it to an MCP error. The other three become `McpError::internal_error` via the existing `format!("{e:#}")` adapter.

## 13. Out of scope (deferred to later plans)

- All write operations other than AddTodo (Plan 5).
- Tag admin operations (Plan 6).
- Recurrence (Plan 7).
- AppleScript path (Plans 6 + 7).
- `x-callback-url` round-trip (rejected as too heavyweight in design spec §5 — polling is sufficient).
- Project create with nested items (deferred; AddTodo without `items[]` is the minimum viable demo).

## 14. Task budget

Estimated 7–8 atomic tasks (one commit each). Final shape determined when the writing-plans skill drafts the per-task breakdown.

- T1: scaffold `core/writer/{mod,operation,outcome}.rs` + Operation::AddTodo + render_json + tests
- T2: `url.rs` + auth-token masking helper + tests
- T3: `executor.rs` (trait + OpenCommandExecutor + RecordingExecutor) + tests
- T4: `verify.rs` (predicate enum + verify + 3 happy-path tests + timeout test)
- T5: `writer.rs` (Writer struct + fire + 3 safety-gate tests)
- T6: Config + AppState wiring (executor_override hook, SafetyMode resolution, WriterConfig section)
- T7: `things_add_todo` tool + adapter + dry-run integration test + recording-executor integration test
- T8: README + final sweep + Plan 4 wrap-up

## 15. Acceptance criteria

- 65 → ~85 tests passing; `cargo build --release` clean.
- `things_add_todo` registered on `ThingsServer` with all four MCP annotations.
- Dry-run integration test asserts no executor call AND `WriteOutcome { dry_run: true }`.
- Recording-executor integration test asserts a single URL recorded AND it parses to a valid `things:///json` URL containing the to-do title.
- `core/writer/` matches the file split in §1 exactly; no file exceeds ~250 lines.
- No `unwrap()` outside test code.
- README status line bumped to Plan 4.
- `THINGS_AUTH_TOKEN` is wrapped in `SecretString` end-to-end; no test or log emits the raw token.

---

**Next step.** Once approved, invoke `superpowers:writing-plans` to draft `docs/superpowers/plans/2026-05-20-plan-4-writer-infra.md`.
