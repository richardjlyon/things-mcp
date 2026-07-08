# things-mcp Plan 8 — HTTP transport for Cowork (port from zotero-mcp)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development.

**Goal:** Land HTTPS-reachable, OAuth-protected, launchd-managed `things-mcp http-server` so Claude.ai Cowork can use the existing 29-tool surface.

**Approach:** Direct port from `/Users/rjl/Code/mcp-zotero/crates/zotero-mcp/src/`. Module shape, file boundaries, auth model, file layout, plist shape — all mirror the reference. Per-task changes are limited to things-mcp-specific identifiers (config dir, plist label, setup-step contents, env var names).

**Spec:** `docs/superpowers/specs/2026-05-21-plan-8-http-cowork-design.md`

**Baseline:** HEAD `e06cae0`, 158 tests reported (156 passing + 2 ignored).

**Expected end state:** ~196 tests reported (~194 passing + 2 ignored), 9 new commits, `things-mcp setup` exits 0, `things-mcp status` reports green, Claude.ai connector configured.

---

## Task 1: Cargo deps + workspace setup

**Files:**
- Modify: `Cargo.toml` (workspace deps)
- Modify: `crates/things-mcp/Cargo.toml`

**Steps:**

- [ ] **Step 1.** Add to `[workspace.dependencies]` in root `Cargo.toml`:

```toml
axum = "0.8"
tower = "0.5"
tower-http = { version = "0.6", features = ["auth"] }
sha2 = "0.10"
hex = "0.4"
url = "2"
rand = "0.8"
axum-test = "17"          # dev-dep
```

- [ ] **Step 2.** Add `"transport-streamable-http-server"` to the rmcp features list in the workspace dep.

- [ ] **Step 3.** Add to `crates/things-mcp/Cargo.toml` dependencies: `axum`, `tower`, `tower-http`, `sha2`, `hex`, `url`, `rand`. Add `axum-test` to dev-dependencies.

- [ ] **Step 4.** `cargo build` — confirm clean.

- [ ] **Step 5.** Commit:

```
cargo: add HTTP/OAuth deps for plan 8 (axum, tower, sha2, url, …)
```

---

## Task 2: Port `oauth/token_store.rs`

**Files:**
- Create: `crates/things-mcp/src/oauth/mod.rs`
- Create: `crates/things-mcp/src/oauth/token_store.rs`
- Modify: `crates/things-mcp/src/lib.rs` (add `pub mod oauth;`)

**Steps:**

- [ ] **Step 1.** Copy `/Users/rjl/Code/mcp-zotero/crates/zotero-mcp/src/oauth/token_store.rs` → `crates/things-mcp/src/oauth/token_store.rs` verbatim.

- [ ] **Step 2.** Scan the copied file for `zotero` / `Zotero` / `com.zotero-mcp` references. Replace with `things` / `Things` / `dev.things-mcp.things-mcp` as appropriate.

- [ ] **Step 3.** Create `crates/things-mcp/src/oauth/mod.rs` with `pub mod token_store;` and any re-exports the zotero-mcp `oauth.rs` head currently imports.

- [ ] **Step 4.** Add `pub mod oauth;` to `crates/things-mcp/src/lib.rs`.

- [ ] **Step 5.** `cargo test --lib oauth::token_store` — confirm all token_store tests pass.

- [ ] **Step 6.** Commit:

```
oauth/token_store: port from zotero-mcp (SHA-256 hashed at rest, 0600)
```

---

## Task 3: Port `bearer.rs`

**Files:**
- Create: `crates/things-mcp/src/bearer.rs`
- Modify: `crates/things-mcp/src/lib.rs`

**Steps:**

- [ ] **Step 1.** Copy `/Users/rjl/Code/mcp-zotero/crates/zotero-mcp/src/bearer.rs` → `crates/things-mcp/src/bearer.rs` verbatim.

- [ ] **Step 2.** Adjust module-level docstring zotero references → things references.

- [ ] **Step 3.** Add `pub mod bearer;` to lib.rs.

- [ ] **Step 4.** `cargo test --lib bearer` — confirm all bearer tests pass.

- [ ] **Step 5.** Commit:

```
bearer: tower-http Authorization: Bearer middleware (port from zotero-mcp)
```

---

## Task 4: Port `oauth.rs` (the big one)

**Files:**
- Create: `crates/things-mcp/src/oauth.rs` (or move into the `oauth/` subdir if zotero-mcp does — verify)

**Steps:**

- [ ] **Step 1.** Inspect zotero-mcp's layout: is `oauth.rs` a single file alongside the `oauth/` subdir, or is it part of the subdir? Mirror that exactly.

- [ ] **Step 2.** Copy verbatim.

- [ ] **Step 3.** Sweep for identifiers needing replacement:
  - `com.zotero-mcp` → `com.things-mcp`
  - Config dir `dev.zotero-mcp.zotero-mcp` → `dev.things-mcp.things-mcp`
  - Default issuer host hint (if hardcoded for examples)
  - `ZOTERO_*` env var names if any leak through
  - Any reference to the Zotero local API base URL — should not exist in this module; flag if found

- [ ] **Step 4.** Adjust the OAuth `authorization_html` template if it mentions "Zotero" by name.

- [ ] **Step 5.** `cargo test --lib oauth` — confirm tests pass.

- [ ] **Step 6.** Commit:

```
oauth: PKCE flow + discovery + token endpoints (port from zotero-mcp)
```

---

## Task 5: Port `http_transport.rs`

**Files:**
- Create: `crates/things-mcp/src/http_transport.rs`

**Steps:**

- [ ] **Step 1.** Copy `/Users/rjl/Code/mcp-zotero/crates/zotero-mcp/src/http_transport.rs` verbatim.

- [ ] **Step 2.** Replace zotero-mcp identifiers in the module's docstring and any constants.

- [ ] **Step 3.** Confirm the `ServerHandler` it expects — should be a generic `Arc<dyn ServerHandler>` parameter (likely), allowing `ThingsServer` to be passed in without changes. If it's typed to zotero's `Server`, generalize.

- [ ] **Step 4.** Add `pub mod http_transport;` to lib.rs.

- [ ] **Step 5.** `cargo build` — confirm.

- [ ] **Step 6.** Commit:

```
http_transport: rmcp StreamableHttpService wiring (port from zotero-mcp)
```

---

## Task 6: Port `setup.rs` with Things-specific adjustments

**Files:**
- Create: `crates/things-mcp/src/setup.rs`

**Steps:**

- [ ] **Step 1.** Copy `/Users/rjl/Code/mcp-zotero/crates/zotero-mcp/src/setup.rs`.

- [ ] **Step 2.** Replace the "detect Zotero" step with a "detect Things 3" step:
  - Probe for `/Applications/Things3.app` existence.
  - Probe the SQLite DB path via `crate::core::reader::schema::probe(&db_path)` (already implemented).
  - On failure: instruct the user to install Things 3 from `https://culturedcode.com/things/`.

- [ ] **Step 3.** Replace the "ask for Zotero auth" step with a "prompt for `THINGS_AUTH_TOKEN`" step. The token is optional; if user skips, writes are disabled (matching today's behaviour). Instruction text references Things 3 → Settings → General → "Enable Things URLs" → Manage.

- [ ] **Step 4.** Replace plist `Label`: `com.zotero-mcp.http` → `com.things-mcp.http`. `Program`: `<cargo-bin>/zotero-mcp http-server` → `<cargo-bin>/things-mcp http-server`. Log path: `~/Library/Logs/com.zotero-mcp.http.log` → `~/Library/Logs/com.things-mcp.http.log`.

- [ ] **Step 5.** Update config-dir references to `dev.things-mcp.things-mcp`.

- [ ] **Step 6.** Add `pub mod setup;` to lib.rs.

- [ ] **Step 7.** `cargo test --lib setup` — confirm.

- [ ] **Step 8.** Commit:

```
setup: things-mcp setup wizard (port from zotero-mcp + Things 3 probe step)
```

---

## Task 7: state.rs — add OAuth state field

**Files:**
- Modify: `crates/things-mcp/src/state.rs`

**Steps:**

- [ ] **Step 1.** Add `pub oauth_state: Option<Arc<crate::oauth::OAuthState>>` field to `AppState` (Option because stdio path doesn't need it).

- [ ] **Step 2.** Wire population in `AppState::build` — when the CLI is `http-server`, build the OAuthState from `oauth.toml` + `config.toml [oauth]` section; otherwise `None`.

- [ ] **Step 3.** Add `[http]` and `[oauth]` config sections to `core/config.rs`. Defaults per the spec.

- [ ] **Step 4.** `cargo test` — confirm all 158 existing tests still pass.

- [ ] **Step 5.** Commit:

```
state/config: oauth_state field + [http]/[oauth] config sections
```

---

## Task 8: main.rs — clap subcommands

**Files:**
- Modify: `crates/things-mcp/src/main.rs`

**Steps:**

- [ ] **Step 1.** Read zotero-mcp's `main.rs` as the canonical shape.

- [ ] **Step 2.** Replace existing `Cli` struct with a `Cli` carrying global options and an optional `command: Option<Command>` field.

- [ ] **Step 3.** Define `Command` enum: `Setup`, `Status`, `ShowCredentials`, `HttpServer`, `Stdio`. Each delegates to a function in the corresponding module.

- [ ] **Step 4.** Default (no subcommand) → same as `Stdio` → preserves today's `claude mcp add` registrations without re-config.

- [ ] **Step 5.** `things-mcp http-server` → calls `http_transport::run_server(state).await`.

- [ ] **Step 6.** `things-mcp setup` → calls `setup::run().await`.

- [ ] **Step 7.** `things-mcp status` → calls `setup::status().await` (or wherever zotero-mcp's status lives).

- [ ] **Step 8.** `things-mcp show-credentials` → reads `oauth.toml`, prints client_id + client_secret + connector URL.

- [ ] **Step 9.** `cargo build && things-mcp --help` — confirm subcommand list shows.

- [ ] **Step 10.** Commit:

```
main: clap subcommands (setup, status, show-credentials, http-server)
```

---

## Task 9: Integration tests

**Files:**
- Create: `crates/things-mcp/tests/end_to_end_http_plan_8.rs`

**Steps:**

- [ ] **Step 1.** Mirror zotero-mcp's integration tests for the HTTP path. Adapt to things-mcp's `ThingsServer`.

- [ ] **Step 2.** Tests to include (minimum):
  - HTTP `tools/list` returns all 29 things tools.
  - Bearer middleware rejects missing/wrong/expired tokens.
  - OAuth `/.well-known/oauth-authorization-server` returns valid discovery.
  - Full PKCE flow: `/authorize` → code → `/token` → access token → bearer-authed `/mcp` call.

- [ ] **Step 3.** `cargo test` — confirm ~196 reported.

- [ ] **Step 4.** Commit:

```
tests: plan 8 HTTP + OAuth + bearer integration coverage
```

---

## Task 10: README + version bump

**Files:**
- Modify: `README.md`
- Modify: `Cargo.toml` (version 0.1.1 → 0.2.0)

**Steps:**

- [ ] **Step 1.** Bump version (minor — new transport is additive but significant).

- [ ] **Step 2.** README: update status line to Plan 8. Replace the "HTTP / Tailscale-Funnel transport (Plan 8 — not yet shipped)" section with shipped documentation: `things-mcp setup`, connector URL, paste creds into Claude.ai.

- [ ] **Step 3.** `cargo test && cargo build --release` — confirm.

- [ ] **Step 4.** Commit:

```
v0.2.0: plan 8 HTTP transport for Cowork shipping
```

- [ ] **Step 5.** Tag + push + crates.io publish:

```bash
git tag -a v0.2.0 -m "Plan 8: HTTP transport for Claude.ai Cowork"
git push origin main v0.2.0
cargo publish -p things-mcp
gh release create v0.2.0 --generate-notes
```

---

## Self-review checklist

- [ ] All 8 zotero-mcp modules ported (token_store, bearer, oauth, http_transport, setup, plus main/state/server adjustments).
- [ ] Zero `zotero` / `Zotero` / `zotero-mcp` references remain anywhere in things-mcp source/config.
- [ ] launchd plist label is `com.things-mcp.http`.
- [ ] Config dir is `~/Library/Application Support/dev.things-mcp.things-mcp/`.
- [ ] `things-mcp` (no subcommand) still works for Claude Code (stdio path unchanged).
- [ ] `things-mcp setup` exits 0 against a real Mac with Things 3 + Tailscale Funnel.
- [ ] `things-mcp status` reports green when the launchd job is bootstrapped.
- [ ] Test count: ~196 reported. No tests broken from Plans 1–6.

When this lands: things-mcp Plan 7 (recurrence) is the natural next step.
