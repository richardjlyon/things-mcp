# Plan 8 — HTTP transport for Cowork (mirror of zotero-mcp)

**Date:** 2026-05-21
**Goal:** Enable Claude.ai's web Cowork sandbox to talk to `things-mcp` over HTTPS via Tailscale Funnel, with OAuth 2.1 + PKCE auth, bearer-middleware-gated `/mcp`, persistent token store, and `launchd` keepalive.

**Authoritative reference:** `/Users/rjl/Code/mcp-zotero/crates/zotero-mcp/src/`. This plan ports the same modules into `things-mcp` with the same shape. No architectural re-litigation.

## Why this is the right approach

- The user runs Things 3 on macOS and uses Claude.ai's Cowork sandbox. Stdio MCP from Claude Code on the Mac works today but doesn't reach Cowork. HTTP+OAuth+Funnel is the proven path; `zotero-connector` ships it.
- A library extraction was considered and deferred — mirroring inline is the explicit user choice ([[feedback-dont-re-litigate-proven-patterns]]).

## Mapping: zotero-mcp → things-mcp

| zotero-mcp source | LOC | things-mcp destination | Adjustments |
|---|---|---|---|
| `src/http_transport.rs` | 153 | `crates/things-mcp/src/http_transport.rs` | Replace zotero refs in module docs |
| `src/bearer.rs` | 149 | `crates/things-mcp/src/bearer.rs` | Direct copy |
| `src/oauth.rs` | 1377 | `crates/things-mcp/src/oauth.rs` | Replace `com.zotero-mcp` label, default issuer host (`things-mcp.<tailnet>`), config path (`dev.things-mcp.things-mcp/`) |
| `src/oauth/token_store.rs` | 700 | `crates/things-mcp/src/oauth/token_store.rs` | Direct copy |
| `src/setup.rs` | 437 | `crates/things-mcp/src/setup.rs` | Replace Zotero-detection step with Things-detection step (probe `/Applications/Things3.app` + SQLite DB path probe — already in `core/reader/schema.rs`); replace plist `Label` and `Program` paths; prompt for `THINGS_AUTH_TOKEN` instead of zotero credentials. Keep all other steps unchanged. |
| `src/main.rs` | 84 | `crates/things-mcp/src/main.rs` (existing 62-line file) | Grow clap subcommands: `setup`, `status`, `show-credentials`, `http-server`. Default (no subcommand) stays stdio. |
| `src/state.rs` | 111 | `crates/things-mcp/src/state.rs` (existing 132 lines) | Add `oauth_state: Option<OAuthState>` field — populated when running HTTP, `None` for stdio. |
| `src/server.rs` | 618 | `crates/things-mcp/src/server.rs` (existing) | No change to tool registrations. Verify `ThingsServer` is `Clone + Send + Sync` (it is — uses `Arc<AppState>`). |

## Configuration changes

`config.toml` grows two sections, mirroring zotero-mcp's:

```toml
[http]
bind = "127.0.0.1"
port = 7892
host_allow_list = []  # populated by setup wizard with the Tailscale Funnel host
sse_keep_alive_secs = 15

[oauth]
issuer = ""                       # set by `things-mcp setup`
access_token_ttl_days = 7
refresh_token_ttl_days = 90
```

Two new files under `~/Library/Application Support/dev.things-mcp.things-mcp/`:

- `oauth.toml` (0600) — `client_id`, `client_secret_hash`, `issuer`.
- `tokens.json` (0600) — issued access + refresh tokens, SHA-256 hashed at rest, expired entries pruned on load.

## Dependencies to add

Same set zotero-mcp uses for HTTP/OAuth. Add to `Cargo.toml` workspace deps:

- `axum = "0.8"`
- `tower = "0.5"`
- `tower-http = { version = "0.6", features = ["auth"] }`
- `sha2 = "0.10"`
- `hex = "0.4"`
- `url = "2"`
- `rand = "0.8"` (if not already present)
- `rmcp` features need `"transport-streamable-http-server"` added to the existing feature list.

Dev deps: `axum-test = "17"`.

## CLI surface (mirrors zotero-mcp)

```text
$ things-mcp --help
Usage: things-mcp [OPTIONS] [COMMAND]

Commands:
  setup              Interactive wizard: probe Things 3, generate OAuth credentials, install launchd plist, enable Tailscale Funnel
  status             Health report: launchd state, HTTP probe, Funnel URL, log tail
  show-credentials   Print the OAuth client_id, client_secret, and connector URL
  http-server        Run the HTTP transport in the foreground (launchd invokes this)
  help

  (no subcommand): stdio MCP server, unchanged from v0.1.1.
```

## launchd plist

Label: `com.things-mcp.http`. Program: `/Users/<you>/.cargo/bin/things-mcp http-server`. `KeepAlive=true`, `RunAtLoad=true`. Logs to `~/Library/Logs/com.things-mcp.http.log`.

The plist template is embedded in the binary via `include_str!`, mirroring zotero-mcp's `setup.rs` approach.

## Tailscale Funnel

Setup step shells out to `tailscale serve funnel <port>` and parses the resulting URL. Same as zotero-mcp. If Funnel isn't granted, surfaces the exact remediation command. The library doesn't depend on Tailscale's Rust SDK — pure shell-out.

## Testing strategy

Tests follow zotero-mcp's pattern:

- Unit tests in each new module (`bearer`, `oauth`, `oauth/token_store`, `setup`, `http_transport`). ~30-40 tests.
- Integration test exercising full HTTP+OAuth+bearer round-trip with a stub handler — ~3 tests.
- All existing stdio tests (currently 156 passing) keep passing.

Target: **~196 reported** total (current 158 + ~38 new), 2 ignored smoke (unchanged).

## What's NOT in Plan 8

- Library extraction. ([[feedback-dont-re-litigate-proven-patterns]])
- Multi-tenant OAuth.
- Token revocation endpoint (RFC 7009).
- Health/metrics endpoint.
- Keychain integration.
- Non-Tailscale-Funnel transports.
- Windows/Linux support.

All explicitly deferred to match zotero-mcp's scope.

## Next step

Plan: `docs/superpowers/plans/2026-05-21-plan-8-http-cowork.md`. Plan structure follows the file mapping above — one task per module ported, in dependency order (token_store → bearer → oauth → http_transport → setup → main wiring → state wiring → integration tests → deploy). Each task is an atomic commit.
