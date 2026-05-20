# things-mcp Plan 1 — Foundation + stdio MCP with `things_list_inbox`

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up the `things-mcp` binary as a stdio MCP server, with the full read pipeline (config, DB-path resolution, startup backup, reader pool, schema probe, AppState) wired through to one working tool — `things_list_inbox` — verified by an end-to-end integration test against a programmatically-built fixture DB.

**Architecture:** Rust workspace at the repo root. Single crate `things-mcp` (binary + library) under `crates/`. `rmcp` over stdio. Reads via `rusqlite` opened read-only and immutable, pooled by a thin `Semaphore` + `spawn_blocking` wrapper (mirrors `zotero-connector`'s pattern; no `deadpool-sqlite`). Auto-backup on startup. Hidden config at `~/Library/Application Support/dev.things-mcp.things-mcp/`. TDD throughout — every code task starts with a failing test.

**Tech Stack:** Rust stable, `rmcp 1.7`, `rusqlite 0.39` (with `backup` feature), `tokio 1`, `schemars 1`, `serde 1`, `clap 4`, `thiserror 2`, `anyhow 1`, `tracing 0.1`, `directories 6`, `toml 1`, `insta 1`.

**Spec:** `docs/superpowers/specs/2026-05-20-things-mcp-server-design.md`

**Follow-on plans:**
- Plan 2: remaining read tools (today / upcoming / anytime / someday / logbook / trash / areas / projects / tags / `get_*` / `list_by_tag`)
- Plan 3: search (`things_search` with FTS5 detection and `LIKE` fallback)
- Plan 4: writer infrastructure (JSON URL builder, dry-run, `open -g`, post-write poll)
- Plan 5: write tools (todos / projects / `assign_tag` / `unassign_tag` / `bulk_json`)
- Plan 6: AppleScript wrapper + tag admin (rename / merge / delete)
- Plan 7: recurrence (experimental)
- Plan 8: HTTP transport + OAuth 2.1 + Tailscale Funnel
- Plan 9: setup / status / show-credentials subcommands + launchd
- Plan 10: docs polish + manual E2E runbook

---

### Task 1: Workspace skeleton + git init

**Files:**
- Create: `/Users/rjl/Code/github/things-mcp-server/.gitignore`
- Create: `/Users/rjl/Code/github/things-mcp-server/Cargo.toml`
- Create: `/Users/rjl/Code/github/things-mcp-server/rust-toolchain.toml`
- Create: `/Users/rjl/Code/github/things-mcp-server/crates/things-mcp/Cargo.toml`
- Create: `/Users/rjl/Code/github/things-mcp-server/crates/things-mcp/src/main.rs`
- Create: `/Users/rjl/Code/github/things-mcp-server/crates/things-mcp/src/lib.rs`

- [ ] **Step 1: Initialise git in the project root**

```bash
cd /Users/rjl/Code/github/things-mcp-server
git init -b main
```

- [ ] **Step 2: Write `.gitignore`**

`/Users/rjl/Code/github/things-mcp-server/.gitignore`:

```
target/
*.swp
.DS_Store
.memsearch/
.cowork/
```

`Cargo.lock` is committed — this is an application binary, not a library.

- [ ] **Step 3: Write `rust-toolchain.toml`**

```toml
[toolchain]
channel = "stable"
components = ["rustfmt", "clippy"]
```

- [ ] **Step 4: Write workspace `Cargo.toml`**

`/Users/rjl/Code/github/things-mcp-server/Cargo.toml`:

```toml
[workspace]
members = ["crates/things-mcp"]
resolver = "2"

[workspace.package]
edition = "2021"
version = "0.1.0"
authors = ["Richard Lyon"]
license = "MIT OR Apache-2.0"
repository = "https://github.com/richardjlyon/things-mcp-server"

[workspace.dependencies]
anyhow = "1"
clap = { version = "4", features = ["derive"] }
directories = "6"
insta = "1"
rmcp = { version = "1.7", features = ["server","macros","schemars","transport-io"] }
rusqlite = { version = "0.39", features = ["bundled","backup"] }
schemars = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
tokio = { version = "1", features = ["full"] }
toml = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter","fmt"] }
tempfile = "3"
```

HTTP-mode features (`transport-streamable-http-server`, `axum`, `tower-http`, `reqwest`, `wiremock`) and OAuth are added in later plans where first needed.

- [ ] **Step 5: Write the crate `Cargo.toml`**

`/Users/rjl/Code/github/things-mcp-server/crates/things-mcp/Cargo.toml`:

```toml
[package]
name = "things-mcp"
edition.workspace = true
version.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true

[lib]
path = "src/lib.rs"

[[bin]]
name = "things-mcp"
path = "src/main.rs"

[dependencies]
anyhow.workspace = true
clap.workspace = true
directories.workspace = true
rmcp.workspace = true
rusqlite.workspace = true
schemars.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
tokio.workspace = true
toml.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true

[dev-dependencies]
insta.workspace = true
tempfile.workspace = true
```

- [ ] **Step 6: Write a minimal `main.rs`**

`/Users/rjl/Code/github/things-mcp-server/crates/things-mcp/src/main.rs`:

```rust
fn main() {
    println!("things-mcp {}", env!("CARGO_PKG_VERSION"));
}
```

- [ ] **Step 7: Write a minimal `lib.rs`**

`/Users/rjl/Code/github/things-mcp-server/crates/things-mcp/src/lib.rs`:

```rust
//! `things-mcp` — local-first MCP bridge between Claude and Things 3.
```

- [ ] **Step 8: Build and run**

```bash
cargo build
cargo run --bin things-mcp
```

Expected stdout: `things-mcp 0.1.0`. Expected build: clean.

- [ ] **Step 9: Initial commit**

```bash
git add .
git commit -m "scaffold: workspace, crate, minimal binary"
```

---

### Task 2: Logging scaffold

**Files:**
- Create: `crates/things-mcp/src/logging.rs`
- Modify: `crates/things-mcp/src/lib.rs`
- Modify: `crates/things-mcp/src/main.rs`

- [ ] **Step 1: Write `logging.rs`**

`crates/things-mcp/src/logging.rs`:

```rust
//! Tracing setup for things-mcp.
//!
//! stderr is always wired; if `log_dir` is provided we also append to
//! `<log_dir>/stdio.log`. Safe to call exactly once per process.

use std::path::Path;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

pub fn init(level: &str, log_dir: Option<&Path>) -> anyhow::Result<()> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(level));

    let stderr_layer = fmt::layer().with_writer(std::io::stderr).with_ansi(false);
    let registry = tracing_subscriber::registry().with(filter).with(stderr_layer);

    if let Some(dir) = log_dir {
        std::fs::create_dir_all(dir)?;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("stdio.log"))?;
        let file_layer = fmt::layer().with_writer(file).with_ansi(false);
        registry.with(file_layer).try_init()?;
    } else {
        registry.try_init()?;
    }
    Ok(())
}
```

- [ ] **Step 2: Update `lib.rs`**

```rust
//! `things-mcp` — local-first MCP bridge between Claude and Things 3.

pub mod logging;
```

- [ ] **Step 3: Update `main.rs`**

```rust
use things_mcp::logging;

fn main() -> anyhow::Result<()> {
    logging::init("info", None)?;
    tracing::info!("things-mcp {} starting", env!("CARGO_PKG_VERSION"));
    Ok(())
}
```

- [ ] **Step 4: Build and run**

```bash
cargo build
cargo run --bin things-mcp
```

Expected: stderr line `INFO things_mcp: things-mcp 0.1.0 starting` (formatting may vary). Exit 0.

- [ ] **Step 5: Commit**

```bash
git add crates/things-mcp/src
git commit -m "logging: tracing init with env-filter and optional file appender"
```

---

### Task 3: `ThingsError` domain enum

**Files:**
- Create: `crates/things-mcp/src/core/mod.rs`
- Create: `crates/things-mcp/src/core/error.rs`
- Modify: `crates/things-mcp/src/lib.rs`

- [ ] **Step 1: Write the failing test**

`crates/things-mcp/src/core/error.rs`:

```rust
//! Domain errors. All variants serialise to a stable structured form so MCP
//! callers see typed errors, never bare strings.

use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ThingsError {
    #[error("missing Things auth-token (set THINGS_AUTH_TOKEN or config.toml [things].auth_token)")]
    MissingAuthToken { hint: String },

    #[error("Things SQLite schema is incompatible — missing columns: {missing:?}")]
    SchemaIncompatible {
        missing: Vec<String>,
        things_version_guess: Option<String>,
    },

    #[error("Things database is locked; retry in {retry_in_ms} ms")]
    DbLocked { retry_in_ms: u32 },

    #[error("write was unverified after {elapsed_ms} ms; payload echo follows")]
    WriteUnverified { payload_echo: String, elapsed_ms: u32 },

    #[error("unsupported recurrence pattern '{pattern}'; supported: {supported:?}")]
    UnsupportedRecurrence { pattern: String, supported: Vec<String> },

    #[error("operation not allowed on repeating item {id} (field '{field}')")]
    OperationNotAllowedOnRepeatingItem { id: String, field: String },

    #[error("dry-run only (test-DB mode): would have opened {url}")]
    DryRun {
        url: String,
        payload: serde_json::Value,
    },

    #[error("Things rejected the auth-token (writes will not succeed)")]
    AuthTokenRejected,

    #[error("AppleScript exited {exit}: {stderr}")]
    AppleScriptFailed { stderr: String, exit: i32 },

    #[error("Things app is not running")]
    ThingsAppNotRunning,

    #[error("invalid input for '{field}': {reason}")]
    InvalidInput { field: String, reason: String },

    #[error("io: {0}")]
    Io(String),

    #[error("sqlite: {0}")]
    Sqlite(String),
}

impl From<std::io::Error> for ThingsError {
    fn from(e: std::io::Error) -> Self { Self::Io(e.to_string()) }
}

impl From<rusqlite::Error> for ThingsError {
    fn from(e: rusqlite::Error) -> Self { Self::Sqlite(e.to_string()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_auth_token_serialises_to_tagged_json() {
        let err = ThingsError::MissingAuthToken { hint: "set THINGS_AUTH_TOKEN".into() };
        let v = serde_json::to_value(&err).unwrap();
        assert_eq!(v["kind"], "missing_auth_token");
        assert_eq!(v["hint"], "set THINGS_AUTH_TOKEN");
    }

    #[test]
    fn schema_incompatible_carries_missing_columns() {
        let err = ThingsError::SchemaIncompatible {
            missing: vec!["TMTask.uuid".into()],
            things_version_guess: None,
        };
        let v = serde_json::to_value(&err).unwrap();
        assert_eq!(v["kind"], "schema_incompatible");
        assert_eq!(v["missing"][0], "TMTask.uuid");
    }
}
```

- [ ] **Step 2: Create `core/mod.rs`**

`crates/things-mcp/src/core/mod.rs`:

```rust
pub mod error;
```

- [ ] **Step 3: Wire `core` into `lib.rs`**

`crates/things-mcp/src/lib.rs`:

```rust
//! `things-mcp` — local-first MCP bridge between Claude and Things 3.

pub mod core;
pub mod logging;
```

- [ ] **Step 4: Run tests**

```bash
cargo test --lib core::error
```

Expected: 2 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/things-mcp/src
git commit -m "core/error: ThingsError enum with serde tagged serialisation"
```

---

### Task 4: Domain types

**Files:**
- Create: `crates/things-mcp/src/core/types.rs`
- Modify: `crates/things-mcp/src/core/mod.rs`

- [ ] **Step 1: Write `types.rs`**

`crates/things-mcp/src/core/types.rs`:

```rust
//! Domain types returned from read tools and write outcomes.
//!
//! Field shapes mirror Things' SQLite columns where it matters (status, type,
//! start) and use friendly Rust enums everywhere else. Dates are surfaced as
//! ISO-8601 strings to keep MCP outputs portable.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Open,
    Canceled,
    Completed,
}

impl TaskStatus {
    pub fn from_sqlite(n: i64) -> Self {
        // Things' TMTask.status: 0=incomplete, 2=canceled, 3=completed
        match n {
            3 => Self::Completed,
            2 => Self::Canceled,
            _ => Self::Open,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    Todo,
    Project,
    Heading,
}

impl TaskKind {
    pub fn from_sqlite(n: i64) -> Self {
        // Things' TMTask.type: 0=todo, 1=project, 2=heading
        match n {
            1 => Self::Project,
            2 => Self::Heading,
            _ => Self::Todo,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StartBucket {
    Inbox,
    Anytime,
    Someday,
}

impl StartBucket {
    pub fn from_sqlite(n: i64) -> Self {
        // Things' TMTask.start: 0=Inbox, 1=Anytime, 2=Someday
        match n {
            1 => Self::Anytime,
            2 => Self::Someday,
            _ => Self::Inbox,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TodoSummary {
    pub id: String,
    pub title: String,
    pub status: TaskStatus,
    pub start: StartBucket,
    pub project_id: Option<String>,
    pub area_id: Option<String>,
    pub heading_id: Option<String>,
    pub tags: Vec<String>,
    pub scheduled: Option<String>,    // ISO-8601 date, decoded from packed integer
    pub deadline: Option<String>,
    pub creation_date: Option<String>, // ISO-8601 datetime, decoded from REAL unix
    pub modification_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TodoFull {
    #[serde(flatten)]
    pub summary: TodoSummary,
    pub notes: Option<String>,
    pub checklist: Vec<ChecklistItem>,
    pub completion_date: Option<String>,
    pub is_repeating_template: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChecklistItem {
    pub id: String,
    pub title: String,
    pub status: TaskStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Project {
    pub id: String,
    pub title: String,
    pub area_id: Option<String>,
    pub status: TaskStatus,
    pub notes: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Area {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Tag {
    pub id: String,
    pub title: String,
    pub parent_id: Option<String>,
    pub shortcut: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WriteOutcome {
    pub id: Option<String>,
    pub action: String,
    pub verified: bool,
    pub dry_run: bool,
    pub latency_ms: u64,
}
```

- [ ] **Step 2: Update `core/mod.rs`**

```rust
pub mod error;
pub mod types;
```

- [ ] **Step 3: Build**

```bash
cargo build
```

Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add crates/things-mcp/src/core
git commit -m "core/types: domain types with schemars derives"
```

---

### Task 5: `Config` struct with TOML round-trip

**Files:**
- Create: `crates/things-mcp/src/core/config.rs`
- Modify: `crates/things-mcp/src/core/mod.rs`

- [ ] **Step 1: Write the failing test**

`crates/things-mcp/src/core/config.rs`:

```rust
//! Persistent configuration.
//!
//! Loaded from `<config_dir>/config.toml` if present; missing file yields
//! a `Config::default()`. `config_dir()` resolves
//! `~/Library/Application Support/dev.things-mcp.things-mcp/` via the
//! `directories` crate on macOS.

use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

const QUALIFIER: &str = "dev";
const ORG: &str = "things-mcp";
const APP: &str = "things-mcp";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub things: ThingsConfig,
    #[serde(default)]
    pub backup: BackupConfig,
    #[serde(default)]
    pub writer: WriterConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThingsConfig {
    #[serde(default)]
    pub db_path: Option<PathBuf>,
    #[serde(default)]
    pub auth_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupConfig {
    pub retain: u32,
    pub directory: Option<PathBuf>,
}
impl Default for BackupConfig {
    fn default() -> Self { Self { retain: 10, directory: None } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriterConfig {
    pub poll_timeout_ms: u64,
    pub poll_interval_ms: u64,
}
impl Default for WriterConfig {
    fn default() -> Self { Self { poll_timeout_ms: 3000, poll_interval_ms: 100 } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
}
impl Default for LoggingConfig {
    fn default() -> Self { Self { level: "info".into() } }
}

pub fn config_dir() -> anyhow::Result<PathBuf> {
    let dirs = ProjectDirs::from(QUALIFIER, ORG, APP)
        .ok_or_else(|| anyhow::anyhow!("could not resolve config dir"))?;
    Ok(dirs.config_dir().to_path_buf())
}

pub fn config_path() -> anyhow::Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

impl Config {
    pub fn load_from(path: &Path) -> anyhow::Result<Self> {
        if !path.exists() { return Ok(Self::default()); }
        let raw = std::fs::read_to_string(path)?;
        let cfg: Self = toml::from_str(&raw)?;
        Ok(cfg)
    }

    pub fn save_to(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
        let raw = toml::to_string_pretty(self)?;
        std::fs::write(path, raw)?;
        // 0600 on unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut p = std::fs::metadata(path)?.permissions();
            p.set_mode(0o600);
            std::fs::set_permissions(path, p)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn missing_file_yields_default() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        let cfg = Config::load_from(&path).unwrap();
        assert_eq!(cfg.backup.retain, 10);
        assert_eq!(cfg.writer.poll_timeout_ms, 3000);
        assert_eq!(cfg.logging.level, "info");
    }

    #[test]
    fn round_trip_preserves_fields() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        let mut cfg = Config::default();
        cfg.things.db_path = Some(PathBuf::from("/tmp/foo.sqlite"));
        cfg.things.auth_token = Some("abc123".into());
        cfg.backup.retain = 5;
        cfg.save_to(&path).unwrap();
        let loaded = Config::load_from(&path).unwrap();
        assert_eq!(loaded.things.db_path, Some(PathBuf::from("/tmp/foo.sqlite")));
        assert_eq!(loaded.things.auth_token.as_deref(), Some("abc123"));
        assert_eq!(loaded.backup.retain, 5);
    }
}
```

- [ ] **Step 2: Update `core/mod.rs`**

```rust
pub mod config;
pub mod error;
pub mod types;
```

- [ ] **Step 3: Run tests**

```bash
cargo test --lib core::config
```

Expected: 2 passed.

- [ ] **Step 4: Commit**

```bash
git add crates/things-mcp/src/core
git commit -m "core/config: TOML round-trip with 0600 perms and defaults"
```

---

### Task 6: DB path resolution (env → cached → glob, self-healing)

**Files:**
- Modify: `crates/things-mcp/src/core/config.rs` (append a new function + tests)

- [ ] **Step 1: Read the file before editing**

```bash
cat crates/things-mcp/src/core/config.rs | tail -5
```

- [ ] **Step 2: Append the resolver to `config.rs`**

Add at the bottom of `core/config.rs` (before `#[cfg(test)] mod tests`):

```rust
/// Where Things keeps its SQLite under the macOS Group Container.
const GROUP_CONTAINER_GLOB: &str =
    "Library/Group Containers/JLMPQHK86H.com.culturedcode.ThingsMac/ThingsData-*/Things Database.thingsdatabase/main.sqlite";

/// Resolve the live Things DB path using the three-tier precedence from the spec:
/// 1. `THINGS_DB_PATH` env var (or explicit override)
/// 2. cached path in `config.toml [things].db_path` if it still exists on disk
/// 3. glob over `~/Library/Group Containers/.../ThingsData-*/...`
///
/// On a successful glob fallback the resolved path is written back to `config`
/// so subsequent starts skip the glob. Returns `Ok((path, was_cache_hit))`.
pub fn resolve_db_path(
    cfg: &mut Config,
    env_override: Option<&Path>,
    home_dir: &Path,
) -> anyhow::Result<(PathBuf, bool)> {
    if let Some(path) = env_override {
        return Ok((path.to_path_buf(), false));
    }
    if let Some(cached) = cfg.things.db_path.as_ref() {
        if cached.exists() {
            return Ok((cached.clone(), true));
        }
        tracing::warn!("cached Things DB path {:?} missing; re-globbing", cached);
    }
    let pattern = home_dir.join(GROUP_CONTAINER_GLOB);
    let resolved = glob_first_match(&pattern)?
        .ok_or_else(|| anyhow::anyhow!(
            "Things SQLite not found under {}",
            pattern.display()
        ))?;
    cfg.things.db_path = Some(resolved.clone());
    Ok((resolved, false))
}

fn glob_first_match(pattern: &Path) -> anyhow::Result<Option<PathBuf>> {
    // Hand-rolled single-level glob: split on the only `*` segment, readdir
    // the parent, return the first match that satisfies the trailing suffix.
    let s = pattern.to_string_lossy().to_string();
    let star_idx = s.find('*').ok_or_else(|| anyhow::anyhow!("pattern has no '*'"))?;
    let last_sep_before_star = s[..star_idx].rfind('/').unwrap();
    let next_sep_after_star = star_idx + s[star_idx..].find('/').unwrap_or(s.len() - star_idx);
    let parent = PathBuf::from(&s[..last_sep_before_star]);
    let prefix = &s[last_sep_before_star + 1..star_idx];
    let suffix_in_segment = &s[star_idx + 1..next_sep_after_star];
    let trailing = &s[next_sep_after_star..];

    if !parent.exists() { return Ok(None); }
    for entry in std::fs::read_dir(&parent)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(prefix) && name.ends_with(suffix_in_segment) {
            let candidate = parent.join(name.as_ref()).join(trailing.trim_start_matches('/'));
            if candidate.exists() {
                return Ok(Some(candidate));
            }
        }
    }
    Ok(None)
}
```

- [ ] **Step 3: Append tests to the existing `mod tests`**

Inside the existing `#[cfg(test)] mod tests { ... }` block, add:

```rust
    #[test]
    fn env_override_wins() {
        let mut cfg = Config::default();
        let tmp = tempdir().unwrap();
        let override_path = tmp.path().join("custom.sqlite");
        std::fs::write(&override_path, b"").unwrap();
        let (p, hit) = resolve_db_path(&mut cfg, Some(&override_path), tmp.path()).unwrap();
        assert_eq!(p, override_path);
        assert!(!hit);
        // env override never populates the cache
        assert!(cfg.things.db_path.is_none());
    }

    #[test]
    fn cached_path_hit_when_file_exists() {
        let tmp = tempdir().unwrap();
        let real = tmp.path().join("real.sqlite");
        std::fs::write(&real, b"").unwrap();
        let mut cfg = Config::default();
        cfg.things.db_path = Some(real.clone());
        let (p, hit) = resolve_db_path(&mut cfg, None, tmp.path()).unwrap();
        assert_eq!(p, real);
        assert!(hit);
    }

    #[test]
    fn glob_fallback_populates_cache() {
        let tmp = tempdir().unwrap();
        let group = tmp.path().join("Library/Group Containers/JLMPQHK86H.com.culturedcode.ThingsMac/ThingsData-deadbeef/Things Database.thingsdatabase");
        std::fs::create_dir_all(&group).unwrap();
        let db = group.join("main.sqlite");
        std::fs::write(&db, b"").unwrap();
        let mut cfg = Config::default();
        let (p, hit) = resolve_db_path(&mut cfg, None, tmp.path()).unwrap();
        assert_eq!(p, db);
        assert!(!hit);
        assert_eq!(cfg.things.db_path.as_deref(), Some(db.as_path()));
    }

    #[test]
    fn stale_cache_triggers_reglob() {
        let tmp = tempdir().unwrap();
        let group = tmp.path().join("Library/Group Containers/JLMPQHK86H.com.culturedcode.ThingsMac/ThingsData-feedface/Things Database.thingsdatabase");
        std::fs::create_dir_all(&group).unwrap();
        let real = group.join("main.sqlite");
        std::fs::write(&real, b"").unwrap();
        let mut cfg = Config::default();
        cfg.things.db_path = Some(PathBuf::from("/does/not/exist.sqlite"));
        let (p, hit) = resolve_db_path(&mut cfg, None, tmp.path()).unwrap();
        assert_eq!(p, real);
        assert!(!hit);
        assert_eq!(cfg.things.db_path.as_deref(), Some(real.as_path()));
    }
```

- [ ] **Step 4: Run tests**

```bash
cargo test --lib core::config
```

Expected: 6 passed (2 existing + 4 new).

- [ ] **Step 5: Commit**

```bash
git add crates/things-mcp/src/core/config.rs
git commit -m "core/config: three-tier DB path resolver with self-healing cache"
```

---

### Task 7: Backup module (atomic SQLite snapshot + retention)

**Files:**
- Create: `crates/things-mcp/src/core/backup.rs`
- Modify: `crates/things-mcp/src/core/mod.rs`

- [ ] **Step 1: Write the failing test**

`crates/things-mcp/src/core/backup.rs`:

```rust
//! Startup backup of the live Things SQLite.
//!
//! Uses the SQLite online-backup API via `rusqlite::backup` rather than
//! filesystem copy — safe to run while Things itself is writing.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{backup, Connection, OpenFlags};

pub struct Backup {
    pub path: PathBuf,
    pub bytes: u64,
}

pub fn snapshot(live_db: &Path, backup_dir: &Path) -> anyhow::Result<Backup> {
    std::fs::create_dir_all(backup_dir)?;
    let stamp = utc_stamp();
    let out = backup_dir.join(format!("things-{stamp}.sqlite"));

    let src = Connection::open_with_flags(
        live_db,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let mut dst = Connection::open(&out)?;
    let backup = backup::Backup::new(&src, &mut dst)?;
    backup.run_to_completion(64, std::time::Duration::from_millis(10), None)?;
    drop(backup);
    drop(dst);
    drop(src);

    let bytes = std::fs::metadata(&out)?.len();
    Ok(Backup { path: out, bytes })
}

pub fn rotate(backup_dir: &Path, retain: u32) -> anyhow::Result<usize> {
    if !backup_dir.exists() { return Ok(0); }
    let mut entries: Vec<_> = std::fs::read_dir(backup_dir)?
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with("things-")
                 && e.file_name().to_string_lossy().ends_with(".sqlite"))
        .collect();
    entries.sort_by_key(|e| e.file_name());
    // oldest first; keep the newest `retain`
    let drop_n = entries.len().saturating_sub(retain as usize);
    for entry in entries.iter().take(drop_n) {
        std::fs::remove_file(entry.path())?;
    }
    Ok(drop_n)
}

fn utc_stamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (y, mo, d, h, mi, s) = unix_to_ymdhms(secs);
    format!("{y:04}{mo:02}{d:02}-{h:02}{mi:02}{s:02}")
}

fn unix_to_ymdhms(unix_secs: u64) -> (i32, u32, u32, u32, u32, u32) {
    let secs = unix_secs as i64;
    let s = secs.rem_euclid(60) as u32;
    let m_total = secs.div_euclid(60);
    let mi = m_total.rem_euclid(60) as u32;
    let h_total = m_total.div_euclid(60);
    let h = h_total.rem_euclid(24) as u32;
    let mut days = h_total.div_euclid(24);
    // 1970-01-01 was a Thursday; compute date by stepping years and months.
    let mut y: i32 = 1970;
    loop {
        let leap = (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0);
        let year_days = if leap { 366 } else { 365 };
        if days < year_days { break; }
        days -= year_days;
        y += 1;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0);
    let months_len = [31, if leap {29} else {28}, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut mo: u32 = 1;
    for len in months_len.iter() {
        if days < *len { break; }
        days -= *len;
        mo += 1;
    }
    let d = (days as u32) + 1;
    (y, mo, d, h, mi, s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn snapshot_copies_a_sqlite_file() {
        let tmp = tempdir().unwrap();
        let src = tmp.path().join("live.sqlite");
        {
            let c = Connection::open(&src).unwrap();
            c.execute_batch("CREATE TABLE t(x INTEGER); INSERT INTO t VALUES (42);").unwrap();
        }
        let dir = tmp.path().join("backups");
        let backup = snapshot(&src, &dir).unwrap();
        assert!(backup.path.exists());
        assert!(backup.bytes > 0);
        // verify the copy is a valid SQLite with the row
        let c = Connection::open(&backup.path).unwrap();
        let v: i64 = c.query_row("SELECT x FROM t", [], |r| r.get(0)).unwrap();
        assert_eq!(v, 42);
    }

    #[test]
    fn rotate_keeps_only_n_newest() {
        let tmp = tempdir().unwrap();
        for i in 0..5 {
            let name = format!("things-2026010{i}-000000.sqlite");
            std::fs::write(tmp.path().join(&name), b"").unwrap();
        }
        let dropped = rotate(tmp.path(), 2).unwrap();
        assert_eq!(dropped, 3);
        let kept: Vec<_> = std::fs::read_dir(tmp.path()).unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(kept.len(), 2);
        assert!(kept.iter().any(|n| n.contains("20260103")));
        assert!(kept.iter().any(|n| n.contains("20260104")));
    }
}
```

- [ ] **Step 2: Update `core/mod.rs`**

```rust
pub mod backup;
pub mod config;
pub mod error;
pub mod types;
```

- [ ] **Step 3: Run tests**

```bash
cargo test --lib core::backup
```

Expected: 2 passed.

- [ ] **Step 4: Commit**

```bash
git add crates/things-mcp/src/core
git commit -m "core/backup: sqlite_backup snapshot + retention rotation"
```

---

### Task 8: Fixture builder (test helper that produces a minimal Things-shaped DB)

**Files:**
- Create: `crates/things-mcp/src/core/reader/mod.rs`
- Create: `crates/things-mcp/src/core/reader/fixture.rs`
- Modify: `crates/things-mcp/src/core/mod.rs`

- [ ] **Step 1: Write the fixture builder**

`crates/things-mcp/src/core/reader/fixture.rs`:

```rust
//! In-code builder for a minimal Things-shaped SQLite, used by tests.
//!
//! Mirrors only the columns we currently query. The real Things schema has
//! many more columns; the reader code never selects `*`, so omissions here
//! don't matter as long as the columns our queries reference are present.

use std::path::Path;

use rusqlite::Connection;

pub fn build_fixture(path: &Path) -> anyhow::Result<()> {
    let c = Connection::open(path)?;
    c.execute_batch(r#"
        CREATE TABLE TMTask (
            uuid TEXT PRIMARY KEY,
            title TEXT,
            type INTEGER,
            status INTEGER,
            trashed INTEGER,
            start INTEGER,
            startDate INTEGER,
            deadline INTEGER,
            stopDate REAL,
            creationDate REAL,
            userModificationDate REAL,
            project TEXT,
            area TEXT,
            heading TEXT,
            notes TEXT,
            rt1_recurrenceRule BLOB,
            "index" INTEGER,
            todayIndex INTEGER
        );
        CREATE TABLE TMArea (uuid TEXT PRIMARY KEY, title TEXT, "index" INTEGER);
        CREATE TABLE TMTag (uuid TEXT PRIMARY KEY, title TEXT, "index" INTEGER, shortcut TEXT, parent TEXT);
        CREATE TABLE TMTaskTag (tasks TEXT, tags TEXT);
        CREATE TABLE Meta (key TEXT PRIMARY KEY, value TEXT);

        INSERT INTO Meta (key, value) VALUES ('databaseVersion', '21');

        INSERT INTO TMArea (uuid, title, "index") VALUES
            ('area-1', 'Personal', 0);

        INSERT INTO TMTag (uuid, title, "index", shortcut, parent) VALUES
            ('tag-errand', 'Errand', 0, NULL, NULL);

        -- Three inbox to-dos (start=0), one completed, two open.
        INSERT INTO TMTask
            (uuid, title, type, status, trashed, start, creationDate, userModificationDate)
        VALUES
            ('todo-1', 'Buy milk',          0, 0, 0, 0, 1715000000.0, 1715000100.0),
            ('todo-2', 'Call the dentist',  0, 0, 0, 0, 1715000200.0, 1715000300.0),
            ('todo-3', 'Pay tax bill',      0, 3, 0, 0, 1714900000.0, 1714900100.0);

        -- One anytime to-do (start=1) inside a project, just to ensure inbox query excludes it.
        INSERT INTO TMTask
            (uuid, title, type, status, trashed, start, project, creationDate, userModificationDate)
        VALUES
            ('todo-4', 'Read RFC 9457', 0, 0, 0, 1, 'proj-1', 1715001000.0, 1715001100.0);

        -- A project.
        INSERT INTO TMTask
            (uuid, title, type, status, trashed, start, area, creationDate, userModificationDate)
        VALUES
            ('proj-1', 'Reading list', 1, 0, 0, 1, 'area-1', 1714000000.0, 1714000100.0);

        -- Tag mapping: todo-2 carries 'Errand'.
        INSERT INTO TMTaskTag (tasks, tags) VALUES ('todo-2', 'tag-errand');
    "#)?;
    Ok(())
}
```

- [ ] **Step 2: Create `core/reader/mod.rs`**

`crates/things-mcp/src/core/reader/mod.rs`:

```rust
//! Read path: SQLite connection pool, schema probe, and typed query helpers.

pub mod fixture;
```

- [ ] **Step 3: Update `core/mod.rs`**

```rust
pub mod backup;
pub mod config;
pub mod error;
pub mod reader;
pub mod types;
```

- [ ] **Step 4: Add a smoke test**

Append to `crates/things-mcp/src/core/reader/fixture.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn fixture_has_expected_inbox_rows() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("test.sqlite");
        build_fixture(&path).unwrap();
        let c = Connection::open(&path).unwrap();
        let n: i64 = c.query_row(
            "SELECT COUNT(*) FROM TMTask WHERE start = 0 AND trashed = 0",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(n, 3);
    }
}
```

- [ ] **Step 5: Run tests**

```bash
cargo test --lib core::reader::fixture
```

Expected: 1 passed.

- [ ] **Step 6: Commit**

```bash
git add crates/things-mcp/src
git commit -m "core/reader/fixture: in-code Things-shaped SQLite for tests"
```

---

### Task 9: Schema probe

**Files:**
- Create: `crates/things-mcp/src/core/reader/schema.rs`
- Modify: `crates/things-mcp/src/core/reader/mod.rs`

- [ ] **Step 1: Write the failing test**

`crates/things-mcp/src/core/reader/schema.rs`:

```rust
//! Run-once schema probe asserting that every column our queries reference
//! exists on disk. Lets us fail fast with a clear message rather than return
//! garbage if a future Things upgrade renames or removes a column.

use std::path::Path;

use rusqlite::{Connection, OpenFlags};

use crate::core::error::ThingsError;

/// (table, column) pairs the read path depends on. Add to this list as new
/// queries land.
const REQUIRED: &[(&str, &[&str])] = &[
    ("TMTask",  &["uuid","title","type","status","trashed","start","project","area","heading","notes","creationDate","userModificationDate","startDate","deadline","stopDate","rt1_recurrenceRule"]),
    ("TMArea",  &["uuid","title"]),
    ("TMTag",   &["uuid","title","shortcut","parent"]),
    ("TMTaskTag", &["tasks","tags"]),
];

pub fn probe(db_path: &Path) -> Result<(), ThingsError> {
    let c = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let mut missing = Vec::new();
    for (table, cols) in REQUIRED {
        let table_cols = list_columns(&c, table)?;
        for col in *cols {
            if !table_cols.iter().any(|t| t.eq_ignore_ascii_case(col)) {
                missing.push(format!("{table}.{col}"));
            }
        }
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(ThingsError::SchemaIncompatible { missing, things_version_guess: None })
    }
}

fn list_columns(c: &Connection, table: &str) -> Result<Vec<String>, rusqlite::Error> {
    let mut stmt = c.prepare(&format!("PRAGMA table_info(\"{}\")", table))?;
    let cols: Result<Vec<String>, _> = stmt
        .query_map([], |r| r.get::<_, String>(1))?
        .collect();
    cols
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reader::fixture::build_fixture;
    use tempfile::tempdir;

    #[test]
    fn probe_passes_on_fixture() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("ok.sqlite");
        build_fixture(&path).unwrap();
        probe(&path).expect("schema probe should pass on fixture");
    }

    #[test]
    fn probe_reports_missing_columns() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("bad.sqlite");
        let c = Connection::open(&path).unwrap();
        // intentionally drop most of the columns
        c.execute_batch("CREATE TABLE TMTask (uuid TEXT);").unwrap();
        let err = probe(&path).unwrap_err();
        match err {
            ThingsError::SchemaIncompatible { missing, .. } => {
                assert!(missing.iter().any(|m| m == "TMTask.title"));
                assert!(missing.iter().any(|m| m == "TMTask.status"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
```

- [ ] **Step 2: Update `core/reader/mod.rs`**

```rust
pub mod fixture;
pub mod schema;
```

- [ ] **Step 3: Run tests**

```bash
cargo test --lib core::reader::schema
```

Expected: 2 passed.

- [ ] **Step 4: Commit**

```bash
git add crates/things-mcp/src/core/reader
git commit -m "core/reader/schema: startup probe for required tables and columns"
```

---

### Task 10: Reader pool

**Files:**
- Create: `crates/things-mcp/src/core/reader/pool.rs`
- Modify: `crates/things-mcp/src/core/reader/mod.rs`

- [ ] **Step 1: Write the failing test**

`crates/things-mcp/src/core/reader/pool.rs`:

```rust
//! Semaphore-throttled, short-lived RO connection pool.
//!
//! Mirrors `zotero-connector`'s pattern: bound concurrent readers with a
//! `tokio::sync::Semaphore`, open a fresh `Connection` per `with_conn` call
//! using URI flags (`mode=ro`, `nolock=1`, `immutable=1`), run the closure
//! inside `spawn_blocking`. Each call picks up Things' latest committed state
//! automatically because the connection lifetime spans only one query.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rusqlite::{Connection, OpenFlags};
use tokio::sync::Semaphore;

use crate::core::error::ThingsError;

pub fn open_read_only(db: &Path) -> Result<Connection, ThingsError> {
    let uri = format!("file:{}?mode=ro&nolock=1&immutable=1", db.to_string_lossy());
    let conn = Connection::open_with_flags(
        &uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_URI,
    )?;
    conn.busy_timeout(std::time::Duration::from_millis(500))?;
    Ok(conn)
}

#[derive(Clone)]
pub struct ReaderPool {
    inner: Arc<Inner>,
}

struct Inner {
    path: PathBuf,
    sem: Semaphore,
}

impl ReaderPool {
    pub async fn new(db_path: PathBuf, max: usize) -> Result<Self, ThingsError> {
        // Validate the path + permissions up front.
        let _probe = open_read_only(&db_path)?;
        Ok(Self {
            inner: Arc::new(Inner { path: db_path, sem: Semaphore::new(max) }),
        })
    }

    pub fn db_path(&self) -> &Path { &self.inner.path }

    pub async fn with_conn<F, R>(&self, f: F) -> Result<R, ThingsError>
    where
        F: FnOnce(&Connection) -> rusqlite::Result<R> + Send + 'static,
        R: Send + 'static,
    {
        let permit = self.inner.sem.acquire().await
            .map_err(|e| ThingsError::Sqlite(format!("semaphore closed: {e}")))?;
        let path = self.inner.path.clone();
        let result = tokio::task::spawn_blocking(move || -> Result<R, ThingsError> {
            let conn = open_read_only(&path)?;
            f(&conn).map_err(ThingsError::from)
        }).await
            .map_err(|e| ThingsError::Sqlite(format!("join: {e}")))?;
        drop(permit);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reader::fixture::build_fixture;
    use tempfile::tempdir;

    #[tokio::test]
    async fn pool_opens_and_runs_a_query() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let n: i64 = pool.with_conn(|c| {
            c.query_row("SELECT COUNT(*) FROM TMTask", [], |r| r.get(0))
        }).await.unwrap();
        assert_eq!(n, 5);
    }

    #[tokio::test]
    async fn pool_caps_concurrency() {
        // Two permits; three concurrent queries should serialise the third.
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let p = pool.clone();
        let h1 = tokio::spawn(async move {
            p.with_conn(|c| c.query_row("SELECT COUNT(*) FROM TMTask", [], |r| r.get::<_, i64>(0))).await
        });
        let p = pool.clone();
        let h2 = tokio::spawn(async move {
            p.with_conn(|c| c.query_row("SELECT COUNT(*) FROM TMTag", [], |r| r.get::<_, i64>(0))).await
        });
        let p = pool.clone();
        let h3 = tokio::spawn(async move {
            p.with_conn(|c| c.query_row("SELECT COUNT(*) FROM TMArea", [], |r| r.get::<_, i64>(0))).await
        });
        for h in [h1, h2, h3] { h.await.unwrap().unwrap(); }
    }
}
```

- [ ] **Step 2: Update `core/reader/mod.rs`**

```rust
pub mod fixture;
pub mod pool;
pub mod schema;
```

- [ ] **Step 3: Run tests**

```bash
cargo test --lib core::reader::pool
```

Expected: 2 passed.

- [ ] **Step 4: Commit**

```bash
git add crates/things-mcp/src/core/reader
git commit -m "core/reader/pool: semaphore-throttled RO pool with URI flags"
```

---

### Task 11: AppState

**Files:**
- Create: `crates/things-mcp/src/state.rs`
- Modify: `crates/things-mcp/src/lib.rs`

- [ ] **Step 1: Write the state module**

`crates/things-mcp/src/state.rs`:

```rust
//! Application state shared across MCP tool invocations.
//!
//! Built once at startup: loads config, resolves the DB path, runs schema
//! probe, takes a startup backup (unless test-DB mode is in effect), and
//! builds the reader pool.

use std::path::PathBuf;
use std::sync::Arc;

use crate::core::{backup, config::{self, Config}, reader::{pool::ReaderPool, schema}};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db_path: PathBuf,
    pub pool: ReaderPool,
    pub test_db_mode: bool,
    pub allow_writes_on_test_db: bool,
}

pub struct AppStateOptions {
    pub env_db_path: Option<PathBuf>,
    pub home_dir: PathBuf,
    pub config_path: PathBuf,
    pub allow_writes_on_test_db: bool,
}

impl AppState {
    pub async fn build(opts: AppStateOptions) -> anyhow::Result<Self> {
        let mut cfg = Config::load_from(&opts.config_path)?;
        let test_db_mode = opts.env_db_path.is_some();

        let (db_path, _hit) = config::resolve_db_path(
            &mut cfg,
            opts.env_db_path.as_deref(),
            &opts.home_dir,
        )?;
        if !test_db_mode {
            // Persist the resolved path back for next start.
            cfg.save_to(&opts.config_path)?;
        }
        schema::probe(&db_path)?;

        if !test_db_mode {
            let backup_dir = cfg.backup.directory.clone().unwrap_or_else(|| {
                config::config_dir().unwrap_or_else(|_| PathBuf::from(".")).join("backups")
            });
            match backup::snapshot(&db_path, &backup_dir) {
                Ok(b) => {
                    tracing::info!("backup ok: {} ({} bytes)", b.path.display(), b.bytes);
                    let dropped = backup::rotate(&backup_dir, cfg.backup.retain)?;
                    if dropped > 0 { tracing::info!("rotated {} old backups", dropped); }
                }
                Err(e) => tracing::warn!("backup failed (continuing): {e:#}"),
            }
        }

        let pool = ReaderPool::new(db_path.clone(), 4).await?;
        Ok(Self {
            config: Arc::new(cfg),
            db_path,
            pool,
            test_db_mode,
            allow_writes_on_test_db: opts.allow_writes_on_test_db,
        })
    }
}
```

- [ ] **Step 2: Wire `state` into `lib.rs`**

`crates/things-mcp/src/lib.rs`:

```rust
//! `things-mcp` — local-first MCP bridge between Claude and Things 3.

pub mod core;
pub mod logging;
pub mod state;
```

- [ ] **Step 3: Build (no new tests yet — covered end-to-end in Task 14)**

```bash
cargo build
```

Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add crates/things-mcp/src
git commit -m "state: AppState (config + db-path resolve + schema probe + backup + pool)"
```

---

### Task 12: `list_inbox` query

**Files:**
- Create: `crates/things-mcp/src/core/reader/queries.rs`
- Modify: `crates/things-mcp/src/core/reader/mod.rs`

- [ ] **Step 1: Write the failing test**

`crates/things-mcp/src/core/reader/queries.rs`:

```rust
//! Typed SQL helpers against the live Things schema. Every query goes through
//! `prepare_cached`; no string interpolation of user input.
//!
//! Date semantics:
//! - `creationDate`, `userModificationDate`, `stopDate` are REAL Unix seconds.
//! - `startDate`, `deadline` are bit-packed integers (handled in later tasks).

use crate::core::error::ThingsError;
use crate::core::reader::pool::ReaderPool;
use crate::core::types::{StartBucket, TaskStatus, TodoSummary};

pub struct ListInboxParams {
    pub include_completed: bool,
    pub limit: u32,
}

impl Default for ListInboxParams {
    fn default() -> Self { Self { include_completed: false, limit: 200 } }
}

pub async fn list_inbox(
    pool: &ReaderPool,
    params: ListInboxParams,
) -> Result<Vec<TodoSummary>, ThingsError> {
    let status_filter: &'static str = if params.include_completed {
        ""
    } else {
        " AND status = 0"
    };
    let sql = format!(
        r#"
        SELECT
            t.uuid, t.title, t.type, t.status, t.start,
            t.project, t.area, t.heading,
            t.creationDate, t.userModificationDate
        FROM TMTask AS t
        WHERE t.trashed = 0
          AND t.type = 0
          AND t.start = 0
          {status_filter}
        ORDER BY t.creationDate DESC
        LIMIT ?1
        "#,
    );
    let limit = params.limit as i64;
    let rows = pool.with_conn(move |c| -> rusqlite::Result<Vec<TodoSummary>> {
        let mut stmt = c.prepare_cached(&sql)?;
        let iter = stmt.query_map([limit], |r| {
            Ok(TodoSummary {
                id: r.get::<_, String>(0)?,
                title: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                status: TaskStatus::from_sqlite(r.get::<_, i64>(3)?),
                start: StartBucket::from_sqlite(r.get::<_, i64>(4)?),
                project_id: r.get::<_, Option<String>>(5)?,
                area_id: r.get::<_, Option<String>>(6)?,
                heading_id: r.get::<_, Option<String>>(7)?,
                tags: Vec::new(),
                scheduled: None,
                deadline: None,
                creation_date: r.get::<_, Option<f64>>(8)?.map(unix_to_iso),
                modification_date: r.get::<_, Option<f64>>(9)?.map(unix_to_iso),
            })
        })?;
        iter.collect()
    }).await?;
    // Tags are joined separately (small N; one extra round-trip is acceptable
    // for the inbox view).
    let ids: Vec<String> = rows.iter().map(|r| r.id.clone()).collect();
    let tag_map = fetch_tags_for_tasks(pool, ids.clone()).await?;
    let mut with_tags = rows;
    for row in with_tags.iter_mut() {
        if let Some(v) = tag_map.get(&row.id) { row.tags = v.clone(); }
    }
    Ok(with_tags)
}

async fn fetch_tags_for_tasks(
    pool: &ReaderPool,
    task_ids: Vec<String>,
) -> Result<std::collections::HashMap<String, Vec<String>>, ThingsError> {
    if task_ids.is_empty() { return Ok(Default::default()); }
    let placeholders = (0..task_ids.len()).map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        r#"
        SELECT tt.tasks, tg.title
        FROM TMTaskTag AS tt
        JOIN TMTag AS tg ON tg.uuid = tt.tags
        WHERE tt.tasks IN ({placeholders})
        ORDER BY tt.tasks, tg.title
        "#,
    );
    let pairs = pool.with_conn(move |c| -> rusqlite::Result<Vec<(String, String)>> {
        let mut stmt = c.prepare_cached(&sql)?;
        let params = rusqlite::params_from_iter(task_ids.iter());
        let iter = stmt.query_map(params, |r| Ok((r.get(0)?, r.get(1)?)))?;
        iter.collect()
    }).await?;
    let mut out: std::collections::HashMap<String, Vec<String>> = Default::default();
    for (task, tag) in pairs {
        out.entry(task).or_default().push(tag);
    }
    Ok(out)
}

fn unix_to_iso(secs: f64) -> String {
    // Minimal ISO-8601 emitter so we don't pull in `chrono` for one helper.
    let s = secs as i64;
    let (y, mo, d, h, mi, sec) = crate::core::backup::__test_only_unix_to_ymdhms(s);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{sec:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reader::{fixture::build_fixture, pool::ReaderPool};
    use tempfile::tempdir;

    #[tokio::test]
    async fn list_inbox_default_excludes_completed() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let rows = list_inbox(&pool, ListInboxParams::default()).await.unwrap();
        // fixture: 3 inbox rows, one of which is status=3 (completed)
        assert_eq!(rows.len(), 2);
        let titles: Vec<_> = rows.iter().map(|r| r.title.as_str()).collect();
        assert!(titles.contains(&"Buy milk"));
        assert!(titles.contains(&"Call the dentist"));
    }

    #[tokio::test]
    async fn list_inbox_with_completed_includes_completed() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let rows = list_inbox(
            &pool,
            ListInboxParams { include_completed: true, limit: 200 },
        ).await.unwrap();
        assert_eq!(rows.len(), 3);
    }

    #[tokio::test]
    async fn list_inbox_attaches_tags() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let rows = list_inbox(&pool, ListInboxParams::default()).await.unwrap();
        let dentist = rows.iter().find(|r| r.title == "Call the dentist").unwrap();
        assert_eq!(dentist.tags, vec!["Errand".to_string()]);
    }
}
```

- [ ] **Step 2: Expose the date helper from `backup.rs`**

The `unix_to_iso` helper in `queries.rs` calls `crate::core::backup::__test_only_unix_to_ymdhms`. Promote that name from `backup.rs`. Open `crates/things-mcp/src/core/backup.rs` and change:

```rust
fn unix_to_ymdhms(unix_secs: u64) -> (i32, u32, u32, u32, u32, u32) {
```

to:

```rust
pub(crate) fn __test_only_unix_to_ymdhms(unix_secs: i64) -> (i32, u32, u32, u32, u32, u32) {
    let unix_secs = unix_secs.max(0) as u64;
```

…and keep the original `let s = secs.rem_euclid(60) ...` body afterwards (the rebinding from `i64` to `u64` is a one-line shim at the top). Update the existing call inside `utc_stamp()` to use the new name:

```rust
let (y, mo, d, h, mi, s) = __test_only_unix_to_ymdhms(secs as i64);
```

- [ ] **Step 3: Update `core/reader/mod.rs`**

```rust
pub mod fixture;
pub mod pool;
pub mod queries;
pub mod schema;
```

- [ ] **Step 4: Run tests**

```bash
cargo test --lib core::reader::queries
cargo test --lib core::backup
```

Expected: 3 passed in queries, 2 passed in backup.

- [ ] **Step 5: Commit**

```bash
git add crates/things-mcp/src
git commit -m "core/reader/queries: list_inbox with tag join and ISO-8601 dates"
```

---

### Task 13: MCP `things_list_inbox` tool

**Files:**
- Create: `crates/things-mcp/src/tools/mod.rs`
- Create: `crates/things-mcp/src/tools/lists.rs`
- Create: `crates/things-mcp/src/server.rs`
- Modify: `crates/things-mcp/src/lib.rs`

- [ ] **Step 1: Write the tool module**

`crates/things-mcp/src/tools/lists.rs`:

```rust
//! Read tools that surface a Things list view.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::core::reader::queries::{list_inbox, ListInboxParams};
use crate::core::types::TodoSummary;
use crate::state::AppState;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct ListInboxArgs {
    /// Cap on returned rows. Defaults to 200.
    #[serde(default)]
    pub limit: Option<u32>,
    /// If true, completed inbox to-dos are also returned. Defaults to false.
    #[serde(default)]
    pub include_completed: Option<bool>,
}

pub async fn things_list_inbox(
    state: AppState,
    args: ListInboxArgs,
) -> anyhow::Result<Vec<TodoSummary>> {
    let params = ListInboxParams {
        include_completed: args.include_completed.unwrap_or(false),
        limit: args.limit.unwrap_or(200),
    };
    let rows = list_inbox(&state.pool, params).await?;
    Ok(rows)
}
```

- [ ] **Step 2: Create `tools/mod.rs`**

```rust
pub mod lists;
```

- [ ] **Step 3: Write the MCP server**

`crates/things-mcp/src/server.rs`:

```rust
//! `rmcp` `ServerHandler` implementation. Tools are registered with
//! `#[tool_router]` and each delegates to a `tools::*` function. Outputs are
//! returned as `Json<T>` — `rmcp` serialises and emits the structured payload.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{Implementation, ServerCapabilities, ServerInfo};
use rmcp::{tool, tool_handler, tool_router, ErrorData as McpError, Json, ServerHandler};

use crate::core::types::TodoSummary;
use crate::state::AppState;
use crate::tools::lists::{things_list_inbox, ListInboxArgs};

#[derive(Clone)]
pub struct ThingsServer {
    pub state: AppState,
}

#[tool_router]
impl ThingsServer {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    #[tool(
        name = "things_list_inbox",
        description = "Return to-dos in the Things Inbox. Read-only.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn tool_list_inbox(
        &self,
        Parameters(args): Parameters<ListInboxArgs>,
    ) -> Result<Json<Vec<TodoSummary>>, McpError> {
        let state = self.state.clone();
        let rows = things_list_inbox(state, args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(rows))
    }
}

#[tool_handler]
impl ServerHandler for ThingsServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("things-mcp", env!("CARGO_PKG_VERSION")))
    }
}
```

- [ ] **Step 4: Wire `server` and `tools` into `lib.rs`**

`crates/things-mcp/src/lib.rs`:

```rust
//! `things-mcp` — local-first MCP bridge between Claude and Things 3.

pub mod core;
pub mod logging;
pub mod server;
pub mod state;
pub mod tools;
```

- [ ] **Step 5: Build**

```bash
cargo build
```

Expected: clean. The API names (`#[tool_router]`, `#[tool_handler]`, `Parameters<T>` from `rmcp::handler::server::wrapper`, `Json<T>`, `ErrorData as McpError`) are verified against rmcp 1.7 and match the established `zotero-connector` patterns. Note: `#[tool_router]` handles registration internally — `ThingsServer` does **not** carry a `tool_router: ToolRouter<Self>` field. `ServerInfo` and `Implementation` are `#[non_exhaustive]` in 1.7, so use the builder form (`ServerInfo::new(caps).with_server_info(Implementation::new(name, version))`) rather than struct-literal construction.

- [ ] **Step 6: Commit**

```bash
git add crates/things-mcp/src
git commit -m "server: ThingsServer + things_list_inbox tool wired to AppState"
```

---

### Task 14: End-to-end test against fixture DB

**Files:**
- Create: `crates/things-mcp/tests/end_to_end_inbox.rs`

- [ ] **Step 1: Write the failing test**

`crates/things-mcp/tests/end_to_end_inbox.rs`:

```rust
//! End-to-end exercise of the read pipeline: build a fixture DB, build
//! AppState pointed at it, call the tool function the MCP server delegates to,
//! assert the returned shape.
//!
//! This deliberately tests the library API (one rung below the MCP transport
//! layer) — the MCP wiring is a thin shim verified manually via Claude Code.

use things_mcp::core::reader::fixture::build_fixture;
use things_mcp::state::{AppState, AppStateOptions};
use things_mcp::tools::lists::{things_list_inbox, ListInboxArgs};

#[tokio::test]
async fn lists_inbox_against_fixture() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("things.sqlite");
    build_fixture(&db).unwrap();

    // home_dir + config_path are unused when env_db_path is set, but build()
    // still expects them to be present.
    let state = AppState::build(AppStateOptions {
        env_db_path: Some(db.clone()),
        home_dir: tmp.path().to_path_buf(),
        config_path: tmp.path().join("config.toml"),
        allow_writes_on_test_db: false,
    }).await.unwrap();
    assert!(state.test_db_mode);

    let rows = things_list_inbox(state.clone(), ListInboxArgs::default()).await.unwrap();
    let titles: Vec<_> = rows.iter().map(|r| r.title.as_str()).collect();
    assert_eq!(rows.len(), 2);
    assert!(titles.contains(&"Buy milk"));
    assert!(titles.contains(&"Call the dentist"));

    let with_completed = things_list_inbox(state, ListInboxArgs {
        limit: None,
        include_completed: Some(true),
    }).await.unwrap();
    assert_eq!(with_completed.len(), 3);
}
```

- [ ] **Step 2: Run the integration test**

```bash
cargo test --test end_to_end_inbox
```

Expected: 1 passed.

- [ ] **Step 3: Run the entire test suite**

```bash
cargo test
```

Expected: all unit + integration tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/things-mcp/tests
git commit -m "tests: end-to-end exercise of read pipeline against fixture"
```

---

### Task 15: Wire stdio transport and verify with a real MCP client

**Files:**
- Modify: `crates/things-mcp/src/main.rs`

- [ ] **Step 1: Update `main.rs` to bootstrap AppState and serve over stdio**

`crates/things-mcp/src/main.rs`:

```rust
use std::path::PathBuf;

use clap::Parser;
use rmcp::ServiceExt;
use things_mcp::{core::config, logging, server::ThingsServer, state::{AppState, AppStateOptions}};

#[derive(Parser)]
#[command(
    name = "things-mcp",
    about = "Local-first MCP bridge for Things 3 — runs as a stdio MCP server by default."
)]
struct Cli {
    /// Override the live Things DB (test/dev use only).
    #[arg(long, value_name = "PATH")]
    db_path: Option<PathBuf>,
    /// Permit writes when --db-path overrides the live DB. Writes are dry-run.
    #[arg(long)]
    allow_writes_on_test_db: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let env_db = cli.db_path.or_else(|| std::env::var_os("THINGS_DB_PATH").map(PathBuf::from));
    let allow_writes = cli.allow_writes_on_test_db
        || std::env::var("THINGS_MCP_ALLOW_WRITES_ON_TEST_DB").ok().as_deref() == Some("1");

    logging::init("info", None)?;
    tracing::info!("things-mcp {} starting (stdio)", env!("CARGO_PKG_VERSION"));

    let home = directories::UserDirs::new()
        .ok_or_else(|| anyhow::anyhow!("could not resolve home directory"))?
        .home_dir()
        .to_path_buf();
    let cfg_path = config::config_path()?;

    let state = AppState::build(AppStateOptions {
        env_db_path: env_db,
        home_dir: home,
        config_path: cfg_path,
        allow_writes_on_test_db: allow_writes,
    }).await?;

    let server = ThingsServer::new(state);
    let transport = (tokio::io::stdin(), tokio::io::stdout());
    let running = server.serve(transport).await?;
    running.waiting().await?;
    Ok(())
}
```

- [ ] **Step 2: Build**

```bash
cargo build --release
```

Expected: clean. Binary at `target/release/things-mcp`.

- [ ] **Step 3: Manual smoke against Claude Code**

Wire `things-mcp` into Claude Code's `~/.claude/config.json` (or run via `claude mcp add`):

```bash
claude mcp add things-mcp /Users/rjl/Code/github/things-mcp-server/target/release/things-mcp
```

Then in a Claude Code session, ask: *"Use the things_list_inbox tool to show my Things inbox."* Expected: a list of inbox items returned as JSON.

Note: this is the first time the server touches the user's live Things DB. The startup backup runs once; verify it exists:

```bash
ls -la ~/Library/Application\ Support/dev.things-mcp.things-mcp/backups/
```

Expected: at least one `things-<stamp>.sqlite` file.

- [ ] **Step 4: Commit**

```bash
git add crates/things-mcp/src/main.rs
git commit -m "main: clap CLI + stdio bootstrap (AppState + ThingsServer)"
```

---

### Task 16: README, CLAUDE.md, plan-1 wrap-up

**Files:**
- Create: `README.md`
- Create: `CLAUDE.md`

- [ ] **Step 1: Write `README.md`**

`/Users/rjl/Code/github/things-mcp-server/README.md`:

```markdown
# things-mcp-server

A local-first MCP server, written in Rust, bridging Claude (Claude Code on the Mac and Claude.ai's Cowork sandbox) to a live Things 3 instance.

**Status:** Plan 1 — foundation + `things_list_inbox` over stdio. See `docs/superpowers/plans/` for the active plan and follow-ons.

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
```

- [ ] **Step 2: Write `CLAUDE.md`**

`/Users/rjl/Code/github/things-mcp-server/CLAUDE.md`:

```markdown
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

`zotero-connector` (`/Users/rjl/Code/github/zotero-connector`) implements the same dual-transport / OAuth / launchd / Tailscale-Funnel pattern this server will adopt in Plans 8 and 9. Mirror its conventions; do not deviate without writing it down first.
```

- [ ] **Step 3: Commit**

```bash
git add README.md CLAUDE.md
git commit -m "docs: README + CLAUDE.md for plan 1"
```

- [ ] **Step 4: Verify the suite still passes end-to-end**

```bash
cargo test
cargo build --release
```

Expected: all tests green, release build clean.

- [ ] **Step 5: Final plan-1 commit (no-op marker)**

```bash
git log --oneline | head -20
```

Expected: a clean history of ~16 small commits, one per task.

---

## Self-review checklist (for the executor)

Once every task is complete, confirm against the spec:

- [ ] Stdio MCP server starts and exposes `things_list_inbox`.
- [ ] Manual call from Claude Code returns inbox to-dos correctly.
- [ ] Backup file appears on first start; rotation keeps last 10.
- [ ] `config.toml` is materialised at `~/Library/Application Support/dev.things-mcp.things-mcp/config.toml` with the resolved DB path on first run.
- [ ] Schema probe correctly fails on a malformed DB (covered by unit test).
- [ ] `THINGS_DB_PATH` override works (covered by `end_to_end_inbox.rs`).
- [ ] Every commit message starts with the module name (`core/...`, `tools/...`, `server`, `state`, `main`, `tests`, `docs`).

When all green, **Plan 2** is ready to start (remaining read tools).
