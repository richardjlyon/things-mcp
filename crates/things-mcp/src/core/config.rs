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
}
