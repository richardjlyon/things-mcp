# Working in this repo

`things-mcp-server` is the Rust implementation of `things-mcp` — a local-first MCP server bridging Claude to a Things 3 instance over stdio (and, in later plans, streamable-HTTP with OAuth 2.1).

## Conventions

- **Superpowers-driven planning.** Non-trivial changes start with a dated `docs/superpowers/specs/<date>-<topic>-design.md` followed by `docs/superpowers/plans/<date>-<topic>.md`. Implementation follows the plan; ad-hoc improvisation is the exception.
- **TDD enforced.** Tests precede implementation. Read pipeline tests use the in-code `core::reader::fixture::build_fixture` helper; write pipeline tests use the dry-run writer (future plans).
- **MCP tool annotations** (`read_only_hint` / `destructive_hint` / `idempotent_hint` / `open_world_hint`) are mandatory on new tools.
- **Output shapes** prefer typed `Json<T>` with derived `JsonSchema` over loose `CallToolResult` text.

## Layout

| Path | Purpose |
|---|---|
| `crates/things-mcp/src/tools/` | MCP tool surface |
| `crates/things-mcp/src/core/reader/` | SQLite pool, schema probe, typed queries, fixture builder |
| `crates/things-mcp/src/core/{config,backup,types,error}.rs` | config + safety + domain |
| `crates/things-mcp/src/server.rs` | `#[tool_router]` registrations, `ServerHandler` |
| `docs/superpowers/specs/` | per-change design briefs (dated) |
| `docs/superpowers/plans/` | per-change execution plans (dated) |

## Reference repo

`zotero-connector` (`/Users/rjl/Code/mcp-zotero`) implements the same dual-transport / OAuth / launchd / Tailscale-Funnel pattern this server adopted in Plan 8. Mirror its conventions; do not deviate without writing it down first.

**Mirror tax.** The Plan 8 modules (`bearer.rs`, `oauth.rs`, `oauth/token_store.rs`, `http_transport.rs`, `setup.rs`) are inline copies of zotero-mcp's, by deliberate choice — no shared library extraction (pinned principle). The cost: any fix to one of those modules MUST be cherry-picked to the sister repo in the same session, or the repos silently drift. We discovered this the hard way with the `/.well-known/openid-configuration` alias in v0.2.1.

## Project knowledge

Project knowledge (history, decisions, status) lives in the owner's private knowledge base — sessions with access should read the project note there before non-trivial changes. This file carries only how-to-work-here rules.
