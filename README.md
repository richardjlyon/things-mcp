# things-mcp-server

A local-first MCP server, written in Rust, bridging Claude (Claude Code on the Mac and Claude.ai's Cowork sandbox) to a live Things 3 instance.

**Status:** Plan 2 — full read surface (`inbox`/`today`/`upcoming`/`anytime`/`someday`/`logbook`/`trash`/`areas`/`projects`/`tags`/`get_todo`/`get_project`/`list_by_tag`) over stdio. See `docs/superpowers/plans/` for the active plan and follow-ons.

**Quick start (stdio, Claude Code on the Mac):**

```
cargo install --path crates/things-mcp
claude mcp add things-mcp $(which things-mcp)
```

In a Claude Code session: *"List my Things inbox."*

**Configuration:**

- DB path: auto-detected on first run; cached in `~/Library/Application Support/dev.things-mcp.things-mcp/config.toml`.
- Override with `THINGS_DB_PATH=/path/to/test.sqlite` or `--db-path` for development against a fixture.
- Writes (future plans) require Things' own URL-scheme auth token in `THINGS_AUTH_TOKEN` or `[things].auth_token` in `config.toml`.

**Safety:**

- Startup backup of the live Things SQLite to `~/Library/Application Support/dev.things-mcp.things-mcp/backups/` (retains the last 10 by default).
- The reader pool opens the DB read-only and immutable — writes go through the Things JSON URL scheme (later plans), never SQL.

**Roadmap:** see `docs/superpowers/plans/2026-05-20-foundation-and-stdio-mcp.md` for Plan 1 and the list of follow-on plans.
