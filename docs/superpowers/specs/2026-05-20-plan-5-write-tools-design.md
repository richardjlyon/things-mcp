# Plan 5 — remaining write tools on the Plan-4 chassis

**Status:** design draft, 2026-05-20

**Goal.** Ship the rest of the write surface on top of the chassis Plan 4 proved: `things_add_project`, `things_update_todo`, `things_update_project`, `things_complete_todo`, `things_cancel_todo`, `things_move_todo`, and `things_bulk_json`. The chassis (`core/writer/`, `AppState.writer`, `tools/*` adapter pattern, dry-run integration testing) stays as-is — Plan 5 mostly grows the `Operation` enum, extends `VerifyPredicate`, and registers 7 new MCP tools.

**Why this scope.** Plan 4's chassis was deliberately one-tool to prove the round-trip end-to-end: typed payload → JSON URL → executor → bounded SQLite poll → `WriteOutcome`. Every primitive needed by the remaining tools already lives in `core/writer/`; what's left is mechanical fan-out plus a small number of carefully-chosen extensions. Landing all 7 tools in one plan keeps the chassis test surface coherent and gets the auth-token path exercised end-to-end for the first time (Plan 4 wired the gate but never tripped it).

**Predecessor.** `docs/superpowers/plans/2026-05-20-plan-4-writer-infra.md` (shipped, HEAD `78ab8b9`). Test count: 87.

**Parent design.** `docs/superpowers/specs/2026-05-20-things-mcp-server-design.md` §5 (write path) + §6 (tool catalog).

**Reference repo.** None for the JSON-URL surface — Things' JSON URL scheme is the source of truth (`https://culturedcode.com/things/support/articles/2803573`).

---

## 1. Scope at a glance

7 new MCP tools, mapped to 7 new `Operation` enum variants (plus one umbrella variant for bulk):

| Tool | Operation variant | Auth required | Verify | MCP `destructive_hint` / `idempotent_hint` |
|---|---|---|---|---|
| `things_add_project` | `AddProject(AddProjectSpec)` | no | `CreateByTitle` (type=1) | false / false |
| `things_update_todo` | `UpdateTodo(UpdateTodoSpec)` | **yes** | `UpdateById` | false / false |
| `things_update_project` | `UpdateProject(UpdateProjectSpec)` | **yes** | `UpdateById` (project type) | false / false |
| `things_complete_todo` | `CompleteTodo { id }` | **yes** | `StatusChange { Completed }` | false / **true** |
| `things_cancel_todo` | `CancelTodo { id }` | **yes** | `StatusChange { Canceled }` | false / **true** |
| `things_move_todo` | `MoveTodo { id, list_id }` | **yes** | `MoveById` (new) | false / false |
| `things_bulk_json` | `BulkRaw(Vec<serde_json::Value>)` | conditional | none (skip) | **true** / false |

All tools have `read_only_hint=false` and `open_world_hint=true`. `idempotent_hint=true` is reserved for status-change tools (re-completing or re-cancelling a row is a no-op against the SQLite state).

`things_bulk_json` is a power tool: it accepts a raw array of Things JSON operation objects and pipes them through the existing URL builder unchanged. No payload validation beyond "is it valid JSON?" because the surface area of the JSON scheme is wide and the LLM has the scheme documentation in scope. Auth-token is sent unconditionally if configured, so updates work; creates pass through harmlessly when the token is unused. `destructive_hint=true` flags it as the one tool an LLM should defer to user confirmation on.

## 2. Architecture deltas

The Plan 4 chassis lives in `core/writer/`. Plan 5 makes the following surgical changes:

```
crates/things-mcp/src/core/writer/
├── operation.rs                 grows from 160 → ~400 lines  ── SPLIT (see §3)
│   └── operation/               new submodule directory
│       ├── mod.rs               enum + impl Operation { action_name, requires_auth_token, render_json }
│       ├── add_todo.rs          AddTodoSpec + render (moved from current operation.rs)
│       ├── add_project.rs       AddProjectSpec + render
│       ├── update_todo.rs       UpdateTodoSpec + render with operation="update" + id
│       ├── update_project.rs    UpdateProjectSpec + render
│       ├── status_change.rs     CompleteTodo + CancelTodo (renders update with completed/canceled flag)
│       ├── move_todo.rs         MoveTodo + render
│       └── bulk.rs              BulkRaw(Vec<Value>) + passthrough render
├── verify.rs                    +1 variant: MoveById { id, expected_list_id }
└── writer.rs                    fire() signature: VerifyPredicate → Option<VerifyPredicate>
                                  (None = skip verify, used by bulk)
```

```
crates/things-mcp/src/tools/
├── todos.rs                     +4 adapters: update, complete, cancel, move
├── projects.rs                  +2 adapters: add, update
└── bulk.rs                      NEW — things_bulk_json adapter

crates/things-mcp/src/server.rs  +7 #[tool] registrations
```

No new dependencies. No changes to `AppState`, `Config`, or backup/safety logic. The auth-token machinery (`SecretString`, `requires_auth_token`, the auth gate in `Writer::fire`) is fully reused — Plan 5 just gives it work to do.

## 3. Operation module split

`operation.rs` at 160 lines already implements `AddTodo`. Adding 7 more variants with their `render_*` helpers would push it past 400 lines, violating the chassis's ~250-line target. The cleanest reshape:

```rust
// crates/things-mcp/src/core/writer/operation/mod.rs
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
    pub fn action_name(&self) -> &'static str { /* dispatch */ }
    pub fn requires_auth_token(&self) -> bool { /* false for creates + BulkRaw; true otherwise */ }
    pub fn render_json(&self) -> Value { /* delegate to per-variant fn */ }
}
```

Each per-variant file owns its spec struct, the render function, and unit tests for that shape. The enum + impl block in `mod.rs` does nothing but dispatch.

`CompleteTodo` and `CancelTodo` share `status_change.rs` because their render functions are structurally identical (an update with a single boolean flag) — keeping them together documents the duality.

## 4. New Operation variants — render shapes

Each shape is derived from the Things JSON URL scheme docs (`https://culturedcode.com/things/support/articles/2803573`). The plan tasks must verify these against the live docs before commit; this spec captures the intended shape, not authoritative bytes.

### 4.1 `AddProject(AddProjectSpec)`

```rust
pub struct AddProjectSpec {
    pub title: String,
    pub notes: Option<String>,
    pub when: Option<String>,
    pub deadline: Option<String>,
    pub tags: Vec<String>,
    pub area_id: Option<String>,            // parent area
    pub todos: Vec<AddTodoSpec>,            // nested initial to-dos
    pub headings: Vec<String>,              // initial heading titles
}
```

JSON:

```json
{
  "type": "project",
  "attributes": {
    "title": "Launch website",
    "notes": "...",
    "area-id": "area-1",
    "items": [
      { "type": "heading", "attributes": { "title": "Design" } },
      { "type": "to-do",   "attributes": { "title": "Pick palette" } }
    ]
  }
}
```

Plan-4-style verify: `CreateByTitle` with the extra constraint `type = 1` (project). The existing CreateByTitle SQL filters `type = 0`; this is the one place §5 needs a tweak.

### 4.2 `UpdateTodo(UpdateTodoSpec)` / `UpdateProject(UpdateProjectSpec)`

```rust
pub struct UpdateTodoSpec {
    pub id: String,
    pub title: Option<String>,
    pub notes: Option<String>,
    pub when: Option<String>,
    pub deadline: Option<String>,
    pub tags: Option<Vec<String>>,          // None = leave alone, Some(vec![]) = clear all tags
    pub list_id: Option<String>,            // also implicitly a "move" if changed
    pub completed: Option<bool>,
    pub canceled: Option<bool>,
}
```

JSON:

```json
{
  "type": "to-do",
  "operation": "update",
  "id": "abc-123",
  "attributes": { "title": "New title", "completed": true }
}
```

`requires_auth_token()` returns `true` for updates. The auth gate in `Writer::fire` is the only thing standing between the LLM and a confusing "no error, but nothing changed" outcome — `MissingAuthToken` returns immediately rather than firing a URL Things will silently reject.

Verify: `UpdateById { id, expected_title, expected_notes }` — same as Plan 4. Optional fields populate the predicate; absent fields are skipped in the post-write check (best-effort verification).

### 4.3 `CompleteTodo { id }` / `CancelTodo { id }`

Both render as a tiny update with one boolean:

```json
{ "type": "to-do", "operation": "update", "id": "abc", "attributes": { "completed": true } }
```

(Or `"canceled": true` for cancel.) These deserve their own variants — even though the render function is one-liner — because (a) the verify predicate is `StatusChange`, not `UpdateById`; (b) the MCP `idempotent_hint=true`; (c) the tool layer doesn't accept arbitrary attribute updates.

### 4.4 `MoveTodo(MoveTodoSpec)`

```rust
pub struct MoveTodoSpec {
    pub id: String,
    pub list_id: Option<String>,            // Some("proj-id") = move to project/area;
                                            // None        = move to Inbox (no parent)
}
```

JSON:

```json
{ "type": "to-do", "operation": "update", "id": "abc", "attributes": { "list-id": "proj-1" } }
```

`list-id` accepts a project UUID, an area UUID, or — per Things' scheme — the literal string `"inbox"` for the Inbox. The tool layer maps `None` to the inbox sentinel; the spec keeps the type as `Option<String>` to make the "no parent" case explicit at the tool boundary.

Verify: a new `MoveById { id, expected_list_id: Option<String> }` predicate (see §5).

### 4.5 `BulkRaw(BulkRawSpec)`

```rust
pub struct BulkRawSpec {
    pub operations: Vec<serde_json::Value>,
}
```

Pass-through render: each element becomes one JSON object in the URL's payload array. The chassis already calls `build_url(&[op], auth)`; bulk's `render_json` returns the entire array element at once, and `build_url` is extended to accept a flat array OR detect the bulk variant and flatten. The cleaner approach: keep `render_json -> Value` returning one object normally, add `Operation::render_batch(&self) -> Vec<Value>` that returns a vec, and have `build_url` flatten across all input operations.

Auth requirement is conditional: if any element has `"operation": "update"` we'd need the token, but inspecting the payload is fragile. Pragmatic rule: `BulkRaw.requires_auth_token() = true` IF the chassis has a token configured at all (otherwise pass through). Tool-layer documentation makes the trade-off explicit.

Verify: `None` (skip). `BulkRaw` returns `WriteOutcome { id: None, action: "bulk_json", verified: false, dry_run: …, latency_ms }` after the executor call.

## 5. VerifyPredicate extensions

```rust
pub enum VerifyPredicate {
    CreateByTitle { title: String, since_unix: f64, kind: TaskKind },  // NEW: kind disambiguates todo/project
    UpdateById { id: String, expected_title: Option<String>, expected_notes: Option<String> },
    StatusChange { id: String, want: TaskStatus },
    MoveById { id: String, expected_list_id: Option<String> },  // NEW
}
```

Two changes:

1. **`CreateByTitle.kind`** — Plan 4's variant hardcoded `type = 0` (to-do). Adding a `kind: TaskKind` field (existing enum: `Todo / Project / Heading`) lets `AddProject` reuse the same variant without duplication. SQL becomes parameterised on kind.
2. **`MoveById`** — a small variant for the move tool. Checks `t.project IS ? OR t.area IS ?` against `expected_list_id` (or both NULL for inbox).

`Writer::fire`'s signature changes to `Option<VerifyPredicate>` to accommodate `BulkRaw` skipping verify entirely:

```rust
pub async fn fire(
    &self,
    op: Operation,
    verify_pred: Option<VerifyPredicate>,   // None = skip verify
) -> Result<WriteOutcome, ThingsError>
```

When `verify_pred` is `None`, fire skips step 7 (the verify call) and composes `WriteOutcome { verified: false, id: None, dry_run: false, latency_ms }` directly from the executor's elapsed time. All Plan 4 callers update from `pred` to `Some(pred)`.

## 6. Tools

### 6.1 `tools/projects.rs`

```rust
// adds:
pub struct AddProjectArgs { /* mirrors AddProjectSpec, plus a flag for nested items */ }
pub async fn things_add_project(state: AppState, args: AddProjectArgs) -> anyhow::Result<WriteOutcome>;

pub struct UpdateProjectArgs { /* mirrors UpdateProjectSpec */ }
pub async fn things_update_project(state: AppState, args: UpdateProjectArgs) -> anyhow::Result<WriteOutcome>;
```

### 6.2 `tools/todos.rs`

```rust
pub struct UpdateTodoArgs { /* mirrors UpdateTodoSpec */ }
pub async fn things_update_todo(state: AppState, args: UpdateTodoArgs) -> anyhow::Result<WriteOutcome>;

pub struct StatusChangeArgs { pub id: String }
pub async fn things_complete_todo(state: AppState, args: StatusChangeArgs) -> anyhow::Result<WriteOutcome>;
pub async fn things_cancel_todo(state: AppState, args: StatusChangeArgs) -> anyhow::Result<WriteOutcome>;

pub struct MoveTodoArgs { pub id: String, pub list_id: Option<String> }
pub async fn things_move_todo(state: AppState, args: MoveTodoArgs) -> anyhow::Result<WriteOutcome>;
```

### 6.3 `tools/bulk.rs` (new)

```rust
pub struct BulkJsonArgs {
    /// Raw Things JSON URL scheme operation objects.
    /// Each element must be a valid JSON object with the shape Things expects.
    /// Max 250 elements per the Things rate limit.
    pub operations: Vec<serde_json::Value>,
}

pub async fn things_bulk_json(state: AppState, args: BulkJsonArgs) -> anyhow::Result<WriteOutcome>;
```

Pre-condition validation at the tool layer:
- `args.operations.len() > 0` and `<= 250`
- Each element is a JSON object (not a primitive)

Anything beyond that — invalid attribute names, malformed dates, unknown operation types — is Things' problem. The tool's job is to bridge MCP → URL; semantic validation is a Plan 10 polish concern.

## 7. Testing strategy

Plan 4 ships 87 tests. Plan 5 targets ~110 (≈ +20).

| Layer | Test bucket | Count |
|---|---|---|
| `operation/*` per-variant render tests | one minimal + one full per variant | ~14 (7 variants × 2) |
| `verify` MoveById happy path + NotFound | 2 | 2 |
| `writer::fire` with `verify_pred: None` (bulk path) | 1 | 1 |
| Per-tool integration test (dry-run, recording executor asserts URL captured) | 7 | 7 |
| **Total new** | | **~24** |

All integration tests run in dry-run mode against the fixture DB. No live Things app required. The Plan-4 pattern (`build_state(true, Some(recorder))`) extends 1-to-1 — each new integration test asserts the URL was recorded AND the dry-run gate produced `WriteOutcome { dry_run: true }`.

**Auth-token coverage**: Plan 4 wired the auth gate but no test ever exercised the "update + token-present → URL contains auth-token=…" path. Plan 5's `things_update_todo` integration test sets `cfg.things.auth_token = Some("test-token-123")` before building state, then asserts the recorded URL contains `&auth-token=test-token-123`. This is the first end-to-end exercise of the auth path.

## 8. Errors

No new `ThingsError` variants. Existing variants cover the new tools:

- `MissingAuthToken { hint }` — already trips for update tools when token absent
- `InvalidInput { field, reason }` — for bulk's pre-condition checks
- `ExecutorFailed { message }` — propagates from any tool
- `TestDbWriteForbidden` — applies uniformly to all writes

Bulk's "Things rejected the URL" failure mode is invisible to the chassis (no verify, no error from `open`). Documented as a known limitation in the tool's description: callers needing strict verification should use individual tools.

## 9. Out of scope

- Tag CRUD (`things_assign_tag`, `things_create_tag`, `things_rename_tag`, `things_merge_tag`, `things_delete_tag`) — **Plan 6**. Requires AppleScript fallback for ops the JSON scheme doesn't cover (creating tags, deleting tags).
- Recurrence definition — **Plan 7**. AppleScript-only; experimental.
- Streamable-HTTP transport + OAuth — **Plan 8**.
- Setup/status/show-credentials subcommands — **Plan 9**.
- Backwards-compatibility shims for the `verify_pred → Option<verify_pred>` signature change — none needed; all callers are in-tree and updated atomically.

## 10. Task budget

Estimated 8–9 atomic tasks. The plan-writer drafts the breakdown; here's the rough sketch:

- **T1** — `operation/` module split: move `add_todo.rs`, redo `mod.rs` with the empty enum dispatch, all Plan 4 tests still pass.
- **T2** — `add_project.rs` + `AddProject` variant + `CreateByTitle.kind` extension + verify SQL parameterisation + tests.
- **T3** — `update_todo.rs` + `UpdateTodo` variant + tests; bumps `requires_auth_token` to honour update.
- **T4** — `update_project.rs` + `UpdateProject` + tests.
- **T5** — `status_change.rs` + `CompleteTodo` + `CancelTodo` variants + tests.
- **T6** — `move_todo.rs` + `MoveTodo` variant + `VerifyPredicate::MoveById` + tests.
- **T7** — `bulk.rs` + `BulkRaw` variant + `Writer::fire` signature change to `Option<VerifyPredicate>` + tests.
- **T8** — tool adapters in `tools/todos.rs` + `tools/projects.rs` + new `tools/bulk.rs` + `server.rs` registrations + 7 integration tests.
- **T9** — README touch + final sweep (≈110 tests + cargo build --release).

T1's "move existing code without changing behaviour" task is mechanical but critical — it must keep all Plan-4 tests green before any new variants land, so the chassis-split risk is isolated.

## 11. Acceptance criteria

- 87 → ~110 tests passing; `cargo build --release` clean.
- All 7 new tools registered on `ThingsServer` with the MCP annotations specified in §1.
- `operation/` directory exists; no file in it exceeds ~250 lines.
- `Writer::fire` signature is `Option<VerifyPredicate>`; all callers updated.
- `VerifyPredicate::CreateByTitle` carries a `kind: TaskKind` field; SQL parameterised.
- `things_update_todo` integration test exercises the auth-token path end-to-end (URL contains the percent-encoded token).
- `things_bulk_json` rejects empty arrays and arrays > 250 with `ThingsError::InvalidInput`; tests cover both edges.
- README status line bumped to Plan 5.
- No `unwrap()` outside test code. No new `ThingsError` variants.

## 12. Risks

- **JSON shape drift.** Things' JSON scheme is documented but not versioned; the render shapes in §4 are derived from the public docs as of 2026-05-20. T2–T7 must each verify the rendered JSON against the docs before commit. Wrong-shape regressions are silent (Things drops invalid keys without erroring).
- **Bulk verify gap.** `things_bulk_json` returns `verified: false` always — the LLM cannot confirm any individual operation landed without re-querying. Documented in the tool description; not solved here.
- **`operation/` split breaks Plan 4 tests.** Mitigation: T1 is "move only, no behaviour changes"; all 87 tests pass before any new variant lands.
- **Verify race on `MoveById`.** Things may update `t.project`/`t.area` separately from `t.userModificationDate`; if the column lags briefly the verify could time out even on a successful move. Mitigation: the bounded poll already handles brief lag; if real-world Things proves laggier than expected, T6 may need to bump `poll_timeout_ms` for moves only.

---

**Next step.** Once approved, invoke `superpowers:writing-plans` to draft `docs/superpowers/plans/2026-05-20-plan-5-write-tools.md`.
