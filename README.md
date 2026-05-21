# things-mcp-server

A local-first MCP server, written in Rust, that bridges Claude to a live Things 3 instance on macOS. Reads run against a read-only SQLite copy; writes go through Things' own `things:///json` URL scheme; tag-admin ops drive Things via AppleScript. No data leaves the machine.

**Status:** Plan 6 shipping — 29 MCP tools across read, search, write, and tag CRUD. stdio transport only. HTTP / Tailscale-Funnel transport for Claude.ai Cowork is designed (Plan 8) but not yet implemented.

## Quick start

```bash
cargo install --path crates/things-mcp
claude mcp add things-mcp $(which things-mcp)
```

For tools that mutate to-dos through the JSON URL scheme (`things_add_todo`, `things_update_todo`, the assign/unassign pair, etc.), pass the auth token at registration time:

```bash
# Get the token from Things 3 → Settings → General → "Enable Things URLs" → Manage
claude mcp add things-mcp $(which things-mcp) --env THINGS_AUTH_TOKEN=<token>
```

Tag-admin tools (`things_create_tag`, `rename`, `merge`, `delete`, `move`) go via `/usr/bin/osascript` and don't use the auth token. The first call will prompt for macOS Automation permission for `things-mcp` → Things 3.

To upgrade after pulling new commits:

```bash
cargo install --path crates/things-mcp --force
# then /mcp → reconnect things-mcp inside Claude Code
```

## Tool reference

Every tool returns structured JSON (an object — never a bare array). Field shapes are stable across versions; see `crates/things-mcp/src/core/types.rs` for the exact derives. Tool annotations (`read_only_hint`, `destructive_hint`, `idempotent_hint`, `open_world_hint`) are set per the MCP spec so the client can show appropriate UI affordances.

### Reading lists

All list tools return `{ items: [...] }` and accept an optional `limit`. Examples are written as the prompt you'd send to Claude — Claude picks the matching tool.

| Tool | When to reach for it | Example prompt |
|---|---|---|
| `things_list_inbox` | First-look triage; capture review. | *"What's in my Things inbox?"* — or *"Show me everything in inbox including ones I've already checked off this week."* (with `include_completed: true`) |
| `things_list_today` | Daily standup; ad-hoc "what am I doing now?" | *"What's on Today?"* — or *"Read me my Today list and group by project."* |
| `things_list_upcoming` | Weekly review; spotting deadline pile-ups. | *"What's scheduled for the next two weeks?"* — or *"Anything upcoming between 2026-06-01 and 2026-06-15?"* |
| `things_list_anytime` | Backlog scan; pulling work into Today. | *"Show me everything in Anytime under area Personal."* |
| `things_list_someday` | Quarterly review; spotting stale ideas. | *"What's been parked in Someday for a while?"* |
| `things_list_logbook` | Retrospectives; "what did I get done?" | *"What did I complete in the last 7 days?"* — or *"Show me everything I finished between 2026-04-01 and 2026-04-30."* |
| `things_list_trash` | Recovery; "did I delete something useful?" | *"What's in my trash?"* |
| `things_list_areas` | Discover the area set when writing area-scoped queries. | *"List my Things areas."* |
| `things_list_projects` | Find a project ID for follow-up writes; spot stale projects. | *"List all my projects."* — or *"Show me my open projects in the Work area."* |
| `things_list_by_tag` | Cross-cut a view across areas/projects via tag. | *"Show me everything tagged 'Errand' or its children."* — or *"List items tagged 'Phone-call' without recursing into child tags."* (`recurse: false`) |

### Reading tags

| Tool | When to reach for it | Example prompt |
|---|---|---|
| `things_list_tags` | Explore the tag hierarchy; find candidates for merge/cleanup; pick a tag for a write. | *"Show me my full tag tree."* Returns both a `flat` list (every tag with its `parent_id`) and a `roots` tree (each root with nested `children` recursively). |

### Searching

| Tool | When to reach for it | Example prompt |
|---|---|---|
| `things_search` | When list-by-X isn't enough — full-text + multi-filter. Combine free text with structural filters (tags, area, project, status, deadline range, scheduled range). | *"Search my open to-dos for the word 'invoice' in the Finance area."* — or *"Find anything tagged 'Call' due before next Friday."* — or *"Search across open and completed to-dos for 'tax bill' in project Q2-Tax."* |

### Reading single items

| Tool | When to reach for it | Example prompt |
|---|---|---|
| `things_get_todo` | Pull full detail (notes, checklist, dates) for a known UUID. Returns `{ todo: null }` when not found. | *"Open the to-do `<uuid>` and read me its notes and checklist."* |
| `things_get_project` | Pull a project with all its items, headings, and per-heading to-dos. Returns `{ project: null }` when not found. | *"Show me everything inside project `<uuid>`."* |

### Writing to-dos and projects

Every write returns a `WriteOutcome { id?, action, verified, dry_run, latency_ms }`. `verified: true` means the writer polled SQLite and confirmed the write landed. `dry_run: true` means safety mode short-circuited the write (test-DB mode).

| Tool | When to reach for it | Example prompt |
|---|---|---|
| `things_add_todo` | Quick capture; converting a chat into a to-do. Accepts title + optional notes/tags/area/project/deadline/scheduled/list (inbox/anytime/etc). | *"Add a to-do 'Follow up with Sam about Q3 budget' to my Work area, tag it 'Email', and schedule it for tomorrow."* |
| `things_add_project` | Spinning up a multi-step initiative. | *"Create a new project called 'Q3 OKRs Planning' in my Work area, with notes 'Kickoff agenda in shared drive'."* |
| `things_update_todo` | Editing any attribute — title, notes, dates, tags. (Tags replace wholesale; prefer `assign/unassign` for tag deltas.) | *"On to-do `<uuid>`, change the title to 'Draft Q3 budget memo' and move the deadline to 2026-06-20."* |
| `things_update_project` | Editing project attributes. | *"Update project `<uuid>`: set notes to 'Owner: AT; deadline 2026-06-30; deck in /Docs/Q3.'"* |
| `things_complete_todo` | Marking work done. | *"Mark to-do `<uuid>` as completed."* |
| `things_cancel_todo` | Closing out without claiming completion (e.g. obsolete, duplicate). | *"Cancel to-do `<uuid>` — it's a duplicate of `<other-uuid>`."* |
| `things_move_todo` | Reassigning to a different project or area. Pass the destination's UUID as `list_id`, or omit to move to inbox. | *"Move to-do `<uuid>` into project `<project-uuid>`."* |
| `things_bulk_json` | Power tool for batching. Send a raw array of Things JSON URL scheme operation objects (max 250). Skips per-element verify — `verified` is always false. | *"Send this batch: add three to-dos to my Inbox titled 'Pay rent', 'Renew domain', 'Email landlord'."* (Claude assembles the array; this is mostly useful for scripts and migrations, not chat.) |

### Tag write — attach/detach on a to-do

These go through the JSON URL chassis (read-modify-write through `update.tags`).

| Tool | When to reach for it | Example prompt |
|---|---|---|
| `things_assign_tag` | Add one or more tags to a single to-do. Idempotent — re-assigning an already-attached tag is a no-op. | *"Tag to-do `<uuid>` with 'Errand' and 'Phone-call'."* |
| `things_unassign_tag` | Remove one or more tags from a single to-do. Idempotent. | *"Remove the 'Waiting' tag from to-do `<uuid>`."* |

### Tag admin — global tag tree changes

These go through AppleScript (`osascript`). Things 3 must be running; first call triggers macOS Automation permission for `things-mcp` → Things 3.

| Tool | When to reach for it | Example prompt |
|---|---|---|
| `things_create_tag` | New tag, optionally nested under an existing parent. | *"Create a tag 'Deep-work' under 'Focus'."* — or *"Create a root-level tag 'Q3-launch'."* |
| `things_rename_tag` | Cleanup; renaming globally propagates to every to-do that carries the old name. | *"Rename 'Errand' to 'Errands'."* |
| `things_merge_tags` | Tag-tree cleanup: reassign every to-do tagged `source` to also carry `target`, then delete `source`. Source ≠ target. | *"Merge tag 'Errands' into 'Errand' — I have a duplicate."* |
| `things_delete_tag` | Remove a tag globally. The to-dos that carried it stay; only the tag is removed. | *"Delete the tag 'Q3-launch' — that project is over."* |
| `things_move_tag` | Re-nest a tag in the tree. Omit `new_parent` to promote to root. | *"Move 'Deep-work' under 'Focus'."* — or *"Move 'Phone-call' to the root of the tag tree."* |

## Effective patterns

A few high-leverage prompts the tools combine well on:

- **Daily standup.** *"Read me Today and any overdue scheduled items, group by project, and note any with tag 'Urgent'."* — `things_list_today` + `things_search` (scheduled_before today) + post-filter.
- **Weekly review.** *"Walk me through the Logbook for the last 7 days, then Someday items I haven't touched in 90 days, then Upcoming for next 14 days."* — `things_list_logbook` + `things_list_someday` + `things_list_upcoming`.
- **Capture batch from chat.** Paste a list of items into chat; *"Add each of these to my Inbox."* — single `things_add_todo` per line, or `things_bulk_json` for speed.
- **Project kickoff.** *"Create project 'X' under area Work, then add these 6 to-dos under it."* — `things_add_project` then `things_add_todo` × 6 (with `list_id` set to the new project's id).
- **Tag tree spring-clean.** *"Show me my tag tree. Where do you see duplicates or orphans?"* — `things_list_tags` returns the tree; Claude proposes merges; you approve each. Then `things_merge_tags` per pair.
- **Triage at the end of a project.** *"Find every open to-do tagged 'Q3-launch'. For each, tell me the project and any deadline."* — `things_list_by_tag` then per-item context. Optionally `things_unassign_tag` to clean up, or `things_delete_tag` once empty.

## HTTP / Tailscale-Funnel transport (Plan 8 — not yet shipped)

The current binary speaks **stdio MCP only** and runs as a child process of Claude Code on the same Mac. That's sufficient for the desktop CLI but doesn't reach **Claude.ai Cowork** — Anthropic's web sandbox, which sits in a VM that can't open stdio to your laptop.

Plan 8 will add a second transport, mirroring the design of the sibling [`zotero-connector`](https://github.com/rjlcode/zotero-connector):

- **Streamable-HTTP MCP** via `rmcp::StreamableHttpService`, bound to `127.0.0.1` only.
- **Tailscale Funnel** publishes that loopback service at an HTTPS URL like `https://things-mcp.<tailnet>.ts.net`. Tailscale terminates TLS; Funnel limits the inbound surface to your tailnet plus Claude.ai's known egress.
- **OAuth 2.1 + PKCE** in front of the HTTP endpoint. `things-mcp` issues a `client_id` / `client_secret` pair on first run; you paste them into Claude.ai's connector settings. The server enforces a bearer token via tower-http middleware on every request; tokens are SHA-256 hashed at rest under `~/Library/Application Support/dev.things-mcp.things-mcp/` (mode 0600). Default TTLs: 7-day access token, 90-day refresh.
- **launchd plist** keeps the HTTP server alive across reboots and reload cycles.
- **Setup wizard.** `things-mcp setup` will walk through: Things app detection → Funnel URL setup → launchd bootstrap → token generation → self-test. `things-mcp status` and `things-mcp show-credentials` will round out the lifecycle.

The same 29 tools — same handlers, same safety gates, same DryRun mode — are served over both transports. Nothing in `core/` or `tools/` changes between Plan 6 and Plan 8; only the transport seam in `main.rs` grows a second branch.

Until Plan 8 lands, exercise the server via **Claude Code on the Mac**.

## Configuration

`~/Library/Application Support/dev.things-mcp.things-mcp/config.toml` is created on first run:

```toml
[things]
db_path = "/Users/<you>/Library/Group Containers/JLMPQHK86H.com.culturedcode.ThingsMac/Things Database.thingsdatabase/main.sqlite"
# auth_token = "..."  # optional; alternative to THINGS_AUTH_TOKEN env var

[backup]
retain = 10
# directory = "..."  # optional; defaults to <config_dir>/backups

[writer]
poll_timeout_ms = 3000   # max time to confirm a write landed
poll_interval_ms = 100   # poll spacing
```

Environment overrides (useful for development against a fixture DB):

- `THINGS_DB_PATH=/path/to/test.sqlite` — point at a non-live DB. Activates **test-DB mode**: writes are blocked unless you also set `--allow-writes-on-test-db` (in which case they're dry-run).
- `THINGS_AUTH_TOKEN=...` — JSON URL auth token.
- `THINGS_MCP_ALLOW_WRITES_ON_TEST_DB=1` — same as `--allow-writes-on-test-db`.
- `RUST_LOG=things_mcp=debug` — verbose tracing.

CLI flags:

```text
things-mcp --db-path /path/to/test.sqlite --allow-writes-on-test-db
```

## Safety

- **Startup backup.** Every launch snapshots your live Things SQLite to `<config_dir>/backups/things-<timestamp>.sqlite` (retains the last 10 by default). The snapshot uses SQLite's online backup API, not a raw file copy — safe to run while Things is open.
- **Read-only pool.** The reader opens the DB read-only + immutable. Writes never touch SQL — they flow through Things' own JSON URL scheme (the same API the iOS share extension uses).
- **Three safety modes.** `Live` (production), `DryRun` (test-DB + `--allow-writes-on-test-db` — renders the URL/script but doesn't fire), `Forbidden` (test-DB only). Both the JSON URL executor and the AppleScript driver respect the mode.
- **Auth-token redaction.** The token is wrapped in a `secrecy::Secret<String>` so it can't accidentally land in logs or panic messages.

## Development

```bash
cargo test                    # full suite (≈155 tests)
cargo test -- --ignored       # opt-in smoke tests that touch /usr/bin/osascript
cargo build --release         # optimized binary at target/release/things-mcp
```

Project layout follows the conventions in `CLAUDE.md`. Non-trivial changes start with a dated spec in `docs/superpowers/specs/` followed by a plan in `docs/superpowers/plans/`. Each plan task lands as an atomic commit.

## Roadmap

- **Plan 7** — recurrence definition via AppleScript wrapper (the JSON URL scheme can't set recurrence rules).
- **Plan 8** — HTTP transport + OAuth 2.1 + Tailscale Funnel + launchd plist + setup wizard. Unlocks Claude.ai Cowork.
- **Plan 9** — stdio path consolidation and feature parity sweep.

See `docs/superpowers/plans/` for the active plan and follow-ons.
