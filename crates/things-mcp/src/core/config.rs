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
    fn default() -> Self {
        Self {
            retain: 10,
            directory: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriterConfig {
    pub poll_timeout_ms: u64,
    pub poll_interval_ms: u64,
}
impl Default for WriterConfig {
    fn default() -> Self {
        Self {
            poll_timeout_ms: 3000,
            poll_interval_ms: 100,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
}
impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".into(),
        }
    }
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
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(path)?;
        let cfg: Self = toml::from_str(&raw)?;
        Ok(cfg)
    }

    pub fn save_to(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
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
        .ok_or_else(|| anyhow::anyhow!("Things SQLite not found under {}", pattern.display()))?;
    cfg.things.db_path = Some(resolved.clone());
    Ok((resolved, false))
}

fn glob_first_match(pattern: &Path) -> anyhow::Result<Option<PathBuf>> {
    // Hand-rolled single-level glob: split on the only `*` segment, readdir
    // the parent, return the first match that satisfies the trailing suffix.
    let s = pattern.to_string_lossy().to_string();
    let star_idx = s
        .find('*')
        .ok_or_else(|| anyhow::anyhow!("pattern has no '*'"))?;
    let last_sep_before_star = s[..star_idx]
        .rfind('/')
        .ok_or_else(|| anyhow::anyhow!("glob pattern has no '/' before '*'"))?;
    let next_sep_after_star = star_idx + s[star_idx..].find('/').unwrap_or(s.len() - star_idx);
    let parent = PathBuf::from(&s[..last_sep_before_star]);
    let prefix = &s[last_sep_before_star + 1..star_idx];
    let suffix_in_segment = &s[star_idx + 1..next_sep_after_star];
    let trailing = &s[next_sep_after_star..];

    if !parent.exists() {
        return Ok(None);
    }
    for entry in std::fs::read_dir(&parent)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(prefix) && name.ends_with(suffix_in_segment) {
            let candidate = parent
                .join(name.as_ref())
                .join(trailing.trim_start_matches('/'));
            if candidate.exists() {
                return Ok(Some(candidate));
            }
        }
    }
    Ok(None)
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
        assert_eq!(
            loaded.things.db_path,
            Some(PathBuf::from("/tmp/foo.sqlite"))
        );
        assert_eq!(loaded.things.auth_token.as_deref(), Some("abc123"));
        assert_eq!(loaded.backup.retain, 5);
    }

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
}
