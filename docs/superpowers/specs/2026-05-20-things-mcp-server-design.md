# things-mcp-server — Design

**Date:** 2026-05-20
**Status:** Approved (brainstorming)
**Author:** Richard Lyon (with Claude)

## 1. Goal

A local-first MCP server, written in Rust, that bridges Claude (Claude Code on the Mac and Claude.ai's Cowork sandbox) to a live Things 3 instance on the user's machine. The server exposes a comprehensive tool surface: add and update to-dos and projects, show task and project information, full-text and structured search, tag manipulation, and best-effort creation of repeating to-dos.

## 2. Constraints and findings

Findings from analysing the Things URL scheme docs and `ThingsJSONCoder`:

- The Things JSON URL scheme (`things:///json?data=…`) supports `create` and `update` operations on to-dos and projects. Updates require Things' own auth-token.
- The JSON URL scheme is **write-only**: no query/read API exists. Reads must come from elsewhere.
- The JSON URL scheme **cannot define a recurrence rule**. Repeating to-dos can be created only via the Things UI or via AppleScript's `repetition rule` property; several fields cannot be updated on already-repeating items.
- Limits: max 250 items per 10 sec; notes ≤10k characters; URL must be minified and URL-encoded.
- `ThingsJSONCoder` is a Swift create-only model with no IDs, no updates, no recurrence; we use it as a reference for field names but define our own Rust types.
- Things stores its data in `~/Library/Group Containers/JLMPQHK86H.com.culturedcode.ThingsMac/ThingsData-<account-hash>/Things Database.thingsdatabase/main.sqlite` (with FTS5 indices Things maintains). The `ThingsData-<account-hash>` directory is per-install; resolution at startup follows a self-healing precedence: **(1)** `THINGS_DB_PATH` / `--db-path` override, **(2)** `config.toml [things].db_path` if present and the file still exists, **(3)** glob fallback over the Group Container — sub-millisecond on local APFS — and on success the resolved path is written back to `config.toml` so subsequent starts skip the glob. Resolved once at startup; held in `AppState`; never re-resolved per tool call.

Architectural decisions arising from these findings:

| Concern | Decision |
|---|---|
| Reads | SQLite direct, read-only, including FTS5 |
| Writes (main) | Things JSON URL scheme via `open -g` |
| Recurrence + tag admin (rename / merge / delete) | AppleScript via `osascript` |
| Safety | Auto-backup on startup with retention; test-DB swap; dry-run writes in test-DB mode |
| Tool surface | Many narrow MCP tools grouped by entity + one `things_bulk_json` escape hatch |
| Transport | Dual: stdio (default) + streamable-HTTP (when `THINGS_MCP_HTTP` is set), with OAuth 2.1 (auth-code + PKCE) gating the HTTP endpoint |
| Public reachability | Tailscale Funnel publishes the loopback HTTP port to a stable HTTPS URL (mirrors the validated `zotero-connector` deployment) |

## 3. Architecture

### Crate layout

```
things-mcp-server/
├── Cargo.toml                          workspace
├── rust-toolchain.toml
├── README.md
├── CLAUDE.md
├── crates/things-mcp/
│   ├── Cargo.toml                      binary + lib
│   └── src/
│       ├── main.rs                     CLI: default server | setup | status | show-credentials
│       ├── lib.rs
│       ├── server.rs                   rmcp ServerHandler with #[tool_router]
│       ├── http_transport.rs           StreamableHttpService at /mcp
│       ├── bearer.rs                   bearer-token gate (tower layer)
│       ├── oauth.rs / oauth/           OAuth 2.1 (auth-code + PKCE)
│       ├── setup.rs                    setup/status/show-credentials impl
│       ├── state.rs                    AppState (reader pool, writer client, config)
│       ├── logging.rs
│       ├── core/
│       │   ├── config.rs               TOML config + env overrides
│       │   ├── error.rs                thiserror domain errors
│       │   ├── types.rs                Todo, Project, Area, Tag, Heading, ChecklistItem
│       │   ├── reader/                 SQLite reader pool, queries, FTS5
│       │   ├── writer/                 JSON URL builder, opener, post-write SQLite poll
│       │   ├── applescript/            osascript wrapper, recurrence rules, tag admin
│       │   └── backup.rs               startup snapshot + retention
│       └── tools/
│           ├── mod.rs
│           ├── lists.rs                inbox/today/upcoming/etc.
│           ├── search.rs               FTS + structured filters
│           ├── todos.rs                add/update/get/complete/cancel/move
│           ├── projects.rs             add/update/get
│           ├── tags.rs                 assign/unassign/list/rename/merge/delete
│           ├── recurring.rs            AppleScript-backed recurrence (experimental)
│           └── bulk.rs                 things_bulk_json escape hatch
├── docs/
│   ├── CLAUDE_CODE_SETUP.md            stdio wiring
│   ├── CLAUDE_COWORK_SETUP.md          HTTP + Funnel + OAuth wiring
│   ├── launchd/com.things-mcp.http.plist
│   └── superpowers/
│       ├── specs/
│       └── plans/
└── tests/
    ├── fixtures/things-db/             snapshot test DBs (anonymised)
    └── integration/
```

### Transport

Single binary, two modes:

- **stdio (default).** For Claude Code or Claude Desktop running on the same Mac.
- **streamable-HTTP.** Activated when `THINGS_MCP_HTTP=host:port` is set. Mounts `rmcp::StreamableHttpService` at `/mcp`, binds `127.0.0.1` only. Tailscale Funnel publishes that loopback port at `https://<host>.<tailnet>.ts.net/mcp`. OAuth 2.1 gates the public endpoint.

Three CLI subcommands wrap the HTTP deployment:

- `things-mcp setup` — interactive: detect Things app, prompt for Things auth-token, detect Tailscale Funnel, write `~/Library/LaunchAgents/com.things-mcp.http.plist`, bootstrap launchd, enable Funnel, wait for `oauth.toml` to materialise, print credentials block.
- `things-mcp status` — health check across launchd, HTTP listener, Funnel, Things DB readability, auth-token presence, `oauth.toml`.
- `things-mcp show-credentials` — re-prints the connector credentials block.

### Environment

| Variable | Purpose |
|---|---|
| `THINGS_MCP_HTTP` | `host:port` activates HTTP mode |
| `THINGS_MCP_OAUTH_ISSUER` | Public Funnel URL, e.g. `https://laptop.<tailnet>.ts.net`; must match exactly (no trailing slash) |
| `THINGS_MCP_BEARER_TOKEN` | Optional static bearer (development only) |
| `THINGS_MCP_ALLOWED_HOSTS` | Optional comma-separated Host allow-list for DNS-rebinding hardening |
| `THINGS_AUTH_TOKEN` | **Things' own** URL-scheme auth-token; required for writes |
| `THINGS_DB_PATH` | Override the live DB path (test/dev) |
| `THINGS_MCP_ALLOW_WRITES_ON_TEST_DB` | Explicit opt-in to permit dry-run writes when `THINGS_DB_PATH` is set |

### Key dependencies

Versions checked against crates.io 2026-05-20. Where a pin lags the current `max_stable`, the reason is noted.

```toml
rmcp = { version = "1.7", features = ["server","macros","schemars","transport-io","transport-streamable-http-server"] }
rusqlite = { version = "0.39", features = ["bundled","backup"] }
tokio = { version = "1", features = ["full"] }
axum = "0.8"
tower-http = "0.6"
reqwest = { version = "0.13", default-features = false, features = ["rustls","json","gzip","brotli"] }
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
schemars = "1"                                            # required by rmcp 1.7 (^1.0)
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter","fmt"] }
anyhow = "1"
thiserror = "2"
directories = "6"
toml = "1"
insta = "1"            # snapshot testing
wiremock = "0.6"       # dev-dependency, where useful
```

**Pool implementation note.** Earlier drafts pinned `deadpool-sqlite 0.13`. The reference `zotero-connector` instead rolls a ~50-line `Semaphore`+`tokio::task::spawn_blocking` pool that opens a fresh `Connection` (RO + URI + `nolock=1` + `immutable=1`) per call. This sidesteps `deadpool-sqlite`'s version lag (which had pinned us to `rusqlite ^0.38`), keeps connections short-lived (so each picks up Things' latest committed state automatically), and removes a dependency. We adopt that pattern.

## 4. Tool surface

All tools carry MCP annotations (`read_only_hint`, `destructive_hint`, `idempotent_hint`, `open_world_hint`) and return typed `Json<T>` with derived `JsonSchema`.

### Read tools (SQLite, read-only)

| Tool | Purpose | Key inputs |
|---|---|---|
| `things_list_inbox` | Inbox items | `limit?`, `include_completed=false` |
| `things_list_today` | Today list | `include_evening=true` |
| `things_list_upcoming` | Scheduled + deadlined future items | `from?`, `to?` |
| `things_list_anytime` | Anytime list | `area_id?` |
| `things_list_someday` | Someday list | — |
| `things_list_logbook` | Completed/canceled history | `from?`, `to?`, `limit=100` |
| `things_list_trash` | Trashed items | `limit=100` |
| `things_list_areas` | All areas | — |
| `things_list_projects` | Projects, optionally by area | `area_id?`, `status=open\|done\|all` |
| `things_list_tags` | Flat list + hierarchy | — |
| `things_get_todo` | Full to-do incl. checklist, tags, list, heading | `id` |
| `things_get_project` | Full project incl. items, headings | `id`, `include_items=true` |
| `things_list_by_tag` | Items carrying a given tag (recursive option) | `tag`, `recurse=true` |
| `things_search` | FTS5 over titles + notes + structured filters | `query?`, `tags?`, `area_id?`, `project_id?`, `status?`, `due_before?`, `due_after?`, `scheduled_before?`, `scheduled_after?`, `limit=50` |

### Write tools (JSON URL scheme, require `THINGS_AUTH_TOKEN`)

| Tool | Purpose | Notes |
|---|---|---|
| `things_add_todo` | Create to-do | title, notes, when, deadline, tags[], checklist[], list/list_id, heading/heading_id |
| `things_add_project` | Create project | + nested `items[]` (to-dos + headings) |
| `things_update_todo` | Update by id | + `prepend_notes`, `append_notes`, `add_tags`, `prepend_checklist_items`, `append_checklist_items` |
| `things_update_project` | Update by id | + `prepend_notes`, `append_notes`, `add_tags` |
| `things_complete_todo` | Sugar over update | sets `completed=true`; pre-flight read detects repeating templates and refuses with `OperationNotAllowedOnRepeatingItem` (the JSON API silently ignores the field, so client-side detection is required) |
| `things_cancel_todo` | Sugar over update | sets `canceled=true`; same repeating-template pre-flight as complete |
| `things_move_todo` | Re-home to list/heading | sets `list_id` or `heading_id` |
| `things_bulk_json` | Escape hatch | accepts a raw Things JSON array; validated, fired, polled |

### Tag tools

| Tool | Purpose | Transport |
|---|---|---|
| `things_assign_tag` | Add tag(s) to item | JSON URL `add-tags` |
| `things_unassign_tag` | Remove tag(s) from item | JSON URL `tags=[…minus tag]` after read |
| `things_rename_tag` | Rename a tag globally | AppleScript |
| `things_merge_tags` | Merge tag A into B | AppleScript |
| `things_delete_tag` | Delete a tag | AppleScript |
| `things_list_tags` | Hierarchy (read) | SQLite |

### Recurrence (experimental, AppleScript)

| Tool | Purpose | Caveats |
|---|---|---|
| `things_create_repeating_todo` | Create with a repetition rule | Marked `experimental`. Supports daily/weekly/monthly/yearly with interval, weekday-set, end-after-N, end-on-date. Refuses unsupported patterns explicitly. |

### Cross-cutting

- Write tools refuse if `THINGS_AUTH_TOKEN` is absent (structured `MissingAuthToken` error).
- Write tools refuse if `THINGS_DB_PATH` overrides the live DB and `THINGS_MCP_ALLOW_WRITES_ON_TEST_DB` is unset; even when allowed, writes in test mode are dry-run (URL is built and logged, never fired).
- Outputs are typed (`TodoFull`, `TodoSummary`, `Project`, `Area`, `Tag`, `SearchResult`) with auto-derived schemas.

Surface size: ~25 tools (cf. zotero-mcp's ~34).

## 5. Data flow and DB safety

### Read path

```
MCP tool call
   │
   ▼
tools/lists.rs   (validates input, builds query plan)
   │
   ▼
core/reader/    (semaphore pool + spawn_blocking, fresh RO connection per call)
   │
   ├──► main.sqlite        (opened via URI with mode=ro&nolock=1&immutable=1)
   │
   └──► FTS5 virtual tables (titles + notes index Things maintains)
   │
   ▼
core/types::Todo / Project / Tag    (typed row mapping)
   │
   ▼
Json<T>  → MCP CallToolResult
```

- Read pool: 4 connections, opened read-only and immutable to avoid touching WAL.
- All queries live in `core/reader/queries.rs` as named, parameterised `prepare_cached` statements.
- A `schema_probe` runs once at startup, reads `sqlite_master`, and asserts the columns we depend on exist. Schema drift fails fast with a clear error rather than returning garbage.

### Write path

```
MCP write tool call
   │
   ▼
tools/todos.rs (validates input, checks safety gate)
   │
   ▼
core/writer::build_payload  → serde_json::Value (Things JSON array)
   │
   ▼
URL-encode + minify
   │
   ▼
open -g "things:///json?data=<encoded>&auth-token=<token>"   (Command::spawn /usr/bin/open)
   │
   ▼
poll core/reader for the expected change (poll window = poll_timeout_ms)
   ├── create: scan for a new row whose title matches and whose creation_date is ≥ the
   │           write-start timestamp; on hit, fetch full row and return its id
   ├── update: re-read the touched row by id; assert the changed fields match expected
   └── timeout: 3 s default, configurable; returns WriteUnverified error with payload echo
   │
   ▼
Json<WriteOutcome { id, action, verified, latency_ms }>
```

No `x-callback-url` listener — implementing one would require registering as a URL-scheme handler, which is heavyweight. Polling the same SQLite the reader uses is sufficient for verifying the operations we expose. Poll is bounded (max 30 × 100 ms) and includes a final read to populate the response with the canonical post-write state.

Recurrence and tag-admin tools route through `core/applescript/` (`osascript -e` with templated AppleScript snippets) and verify by the same SQLite poll.

### Backups

- On every server start: `sqlite3_backup`-based copy of the live `main.sqlite` (plus `-wal` and `-shm`) to `~/Library/Application Support/dev.things-mcp.things-mcp/backups/things-YYYYMMDD-HHMMSS.sqlite`.
- Retention: last 10 by default (`config.backup.retain`), oldest rotated out. Soft cap ~200 MB total; exceedance logs a warning.
- Skipped when `THINGS_DB_PATH` is overridden (test mode).

### Test DB swap

- `THINGS_DB_PATH=/path/to/test.sqlite` or `--db-path` retargets reads.
- Writes refuse unless `THINGS_MCP_ALLOW_WRITES_ON_TEST_DB=1`. Even then the JSON URL scheme cannot be redirected (`open things:///json` always reaches the live Things app), so writes in test mode are dry-run: the writer builds the URL, logs it verbatim, and returns a synthetic `WriteOutcome{verified=false, dry_run=true}`. Tests can verify URL/JSON construction end-to-end without ever touching the user's real Things data.

### Auth-token discipline

`THINGS_AUTH_TOKEN` is loaded once at startup, kept behind a redacting wrapper, never logged. Absence yields a structured `MissingAuthToken` error from write tools; read tools remain usable.

### Config file

`~/Library/Application Support/dev.things-mcp.things-mcp/config.toml`:

```toml
[things]
db_path = "~/Library/Group Containers/JLMPQHK86H.com.culturedcode.ThingsMac/ThingsData-.../Things Database.thingsdatabase/main.sqlite"  # auto-detected if absent
auth_token = "…"   # alternative to THINGS_AUTH_TOKEN

[backup]
retain = 10
directory = "~/Library/Application Support/dev.things-mcp.things-mcp/backups"

[writer]
poll_timeout_ms = 3000
poll_interval_ms = 100

[logging]
level = "info"
```

## 6. Auth and secrets

### Concern 1 — Things' own auth-token (write authorisation against the Things app)

- Issued by Things itself (Things → Settings → General → Enable Things URLs → Manage).
- Required by the JSON URL scheme whenever an `update` operation is present.
- Source priority: `THINGS_AUTH_TOKEN` env var → `config.toml [things].auth_token` → absent (writes disabled with a clear error).
- Kept behind a redacting wrapper; never logged, never returned over MCP.
- Optional startup self-test: read DB → pick a known stable to-do → issue a no-op `update` (`append_notes=""`). Result logged but non-blocking.

### Concern 2 — OAuth 2.1 (gates the public HTTPS endpoint)

Applies only in HTTP mode. Surface identical to zotero-mcp:

- `GET  /.well-known/oauth-protected-resource`
- `GET  /.well-known/oauth-authorization-server`
- `GET  /authorize`  (PKCE-required, `code_challenge_method=S256`)
- `POST /oauth/token`  (auth-code + refresh-token grants; one-time refresh-token rotation per OAuth 2.1 §4.3.1)
- `POST /mcp`  (bearer-gated via `tower-http` validate-request layer)

Discovery and `/oauth/token` are intentionally unauthenticated so the handshake can complete; only `/mcp` carries the bearer gate.

Persistence:

- `oauth.toml` at `~/Library/Application Support/dev.things-mcp.things-mcp/oauth.toml`, mode `0600`. First HTTP-mode startup generates `client_id = "things-mcp-<8-hex>"`, `client_secret = "<32-hex>"`, `issuer = "<funnel-url>"`. Subsequent starts reuse.
- `tokens.json` at the same dir, mode `0600`, SHA-256-hashed at rest. Survives launchd restarts so Cowork stays connected without re-auth.
- Redirect URIs allow-listed to `https://claude.ai/api/mcp/` and `https://claude.com/api/mcp/` prefixes only.

TTLs:

- Access token 7 days, refresh token 90 days. Both configurable in `oauth.toml`. The 7-day access TTL is a workaround for the same Anthropic proxy refresh-token bug zotero-mcp hit; once fixed, can be reduced.

Dev escape hatch: with `THINGS_MCP_OAUTH_ISSUER` unset and no `oauth.toml`, the HTTP server runs unauthenticated. **Documented as loopback-only**; the setup wizard refuses to enable Funnel in this state.

### Setup wizard (`things-mcp setup`)

```
1.  Detect Things app present + DB readable                  → abort if missing
2.  Detect THINGS_AUTH_TOKEN (env or config.toml); if absent, focus the Things app (`open -a Things3`),
    print the navigation path "Things → Settings → General → Enable Things URLs → Manage",
    and prompt the user to paste the token. Persist to config.toml (mode 0600).
3.  Detect Tailscale + Funnel feature                        → abort with install URL if missing
4.  Resolve Funnel hostname via `tailscale status --json`
5.  Render + write ~/Library/LaunchAgents/com.things-mcp.http.plist (port 8765 default; collision-detect)
6.  launchctl bootstrap gui/$UID …
7.  tailscale funnel --bg 8765
8.  Poll for oauth.toml materialising (server generates on first start)
9.  Print credentials block (client_id / client_secret / server URL)
10. Self-test the public URL (discovery 200, /mcp 401)
```

### Hardening

- HTTP server binds `127.0.0.1` only; LAN access blocked, Funnel is the sole ingress.
- `StreamableHttpServerConfig` defaults: `stateful_mode=true`, `json_response=false` (validated against Cowork in the zotero-mcp rollout).
- Optional `THINGS_MCP_ALLOWED_HOSTS` for DNS-rebinding hardening.
- `oauth.toml`, `tokens.json`, `config.toml` all `0600`.

## 7. Testing, error handling, observability

### Test pyramid

```
                 ┌──────────────────────────────┐
                 │  Manual E2E against live     │   (a few; documented runbook)
                 │  Things app + Cowork URL     │
                 └──────────────────────────────┘
              ┌────────────────────────────────────┐
              │  Integration: stdio server +       │   (per slice; in CI)
              │  fixture DB + dry-run writer       │
              └────────────────────────────────────┘
        ┌──────────────────────────────────────────────┐
        │  Unit: query builders, JSON URL builder,     │   (lots; TDD-first)
        │  AppleScript template renderer, OAuth bits   │
        └──────────────────────────────────────────────┘
```

### Fixture DBs

At `tests/fixtures/things-db/`:

- `empty.sqlite` — fresh-install schema.
- `populated.sqlite` — anonymised snapshot covering every area/project/heading/checklist/tag/recurring shape we exercise.
- `repeating.sqlite` — to-dos with various recurrence rules to exercise read-side decoding.

Generated once via `tools/build_fixture.sh` from a disposable Things test instance, with titles/notes rewritten to lorem. Fixtures are checked in; the build script is reproducible but not run in CI.

### Per-tool tests

Each MCP tool gets a happy-path test and at least one error-path test, exercising the full stack:

- Spin up `ThingsServer::new(AppState)` against a fixture DB.
- For writes: dry-run writer captures the would-be URL; tests assert exact URL string and exact JSON payload.
- For reads: assert response matches an `insta` snapshot.

TDD enforced for every slice (per the zotero-mcp convention): test precedes implementation for transport, query, write-builder, OAuth, and recurrence parser slices.

### Manual E2E runbook (`docs/MANUAL_E2E.md`)

- Stdio: connect Claude Code, run create-todo / complete / search / rename-tag.
- HTTP: through the public Funnel URL via Cowork, same checks plus OAuth round-trip and token refresh.
- Backup verification: confirm a `things-*.sqlite` materialises on startup and rotation works.

### Error handling

Domain errors via `thiserror` in `core/error.rs`. All MCP-facing errors are structured JSON, never bare strings:

```rust
enum ThingsError {
    MissingAuthToken { hint: String },
    SchemaIncompatible { missing: Vec<String>, things_version_guess: Option<String> },
    DbLocked { retry_in_ms: u32 },
    WriteUnverified { payload_echo: String, elapsed_ms: u32 },
    UnsupportedRecurrence { pattern: String, supported: Vec<String> },
    OperationNotAllowedOnRepeatingItem { id: String, field: String },
    DryRun { url: String, payload: serde_json::Value },
    AuthTokenRejected,
    AppleScriptFailed { stderr: String, exit: i32 },
    ThingsAppNotRunning,
    InvalidInput { field: String, reason: String },
}
```

Write tools return either `Json<WriteOutcome>` (success, verified) or a structured `ThingsError` — never a half-success.

### Observability

- `tracing` + `tracing-subscriber` with env filter.
- File log at `~/Library/Logs/things-mcp/{stdio,http}.{log,err.log}`.
- Per-tool spans carry `tool=`, `latency_ms=`, `verified=` fields.
- Auth-token and OAuth secrets always rendered as `<redacted>`.
- `things-mcp status` reads the latest log lines for quick diagnosis.

## 8. Out of scope (YAGNI)

- No web UI; setup is CLI-only.
- No support for non-macOS hosts (Things is Mac/iOS-only; URL scheme + AppleScript + SQLite layout are macOS-specific).
- No multi-tenant: one Things instance per server.
- No "arbitrarily complex recurrence pattern"; supported set is documented and unsupported patterns are refused explicitly.
- No write-path retries on user-level conflicts (e.g., user editing the same to-do in Things UI mid-write). Things is single-source; conflicts surface as poll-mismatches.
- No Things-to-other-system sync (calendar, Reminders, etc.) — pure MCP surface.

## 9. Risks and open questions

| Risk | Mitigation |
|---|---|
| Things schema changes between versions and breaks reads | `schema_probe` at startup; tests against snapshot fixtures pinned to a known Things version; document the supported Things range in README |
| Write-path poll races (Things is slow to commit) | Default 3 s timeout, configurable; `WriteUnverified` error surfaces the unverified payload so the caller can decide |
| AppleScript recurrence surface is incomplete | Tool is marked `experimental`; unsupported patterns refused; coverage documented |
| User edits in Things UI mid-write produce stale poll results | Verified post-write read populates response with canonical state; doc the limitation |
| `oauth.toml` / `tokens.json` leak | `0600` permissions; rotation procedure documented; tokens hashed at rest |
| `open -g` failing silently if Things is not running | Writer probes Things' presence (NSRunningApplication-style check via `osascript`) before firing; returns a clear error |

## 10. Next step

Invoke the writing-plans skill to produce an executable implementation plan, sliced for TDD.
