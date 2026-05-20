//! Application state shared across MCP tool invocations.
//!
//! Built once at startup: loads config, resolves the DB path, runs schema
//! probe, takes a startup backup (unless test-DB mode is in effect), and
//! builds the reader pool.

use std::path::PathBuf;
use std::sync::Arc;

use crate::core::{
    backup,
    config::{self, Config},
    reader::{
        fts::{self, FtsCapability},
        pool::ReaderPool,
        schema,
    },
};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db_path: PathBuf,
    pub pool: ReaderPool,
    pub test_db_mode: bool,
    pub allow_writes_on_test_db: bool,
    pub fts: Option<FtsCapability>,
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

        let (db_path, _hit) =
            config::resolve_db_path(&mut cfg, opts.env_db_path.as_deref(), &opts.home_dir)?;
        if !test_db_mode {
            // Persist the resolved path back for next start.
            cfg.save_to(&opts.config_path)?;
        }
        schema::probe(&db_path)?;

        if !test_db_mode {
            let backup_dir = cfg.backup.directory.clone().unwrap_or_else(|| {
                config::config_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .join("backups")
            });
            match backup::snapshot(&db_path, &backup_dir) {
                Ok(b) => {
                    tracing::info!("backup ok: {} ({} bytes)", b.path.display(), b.bytes);
                    let dropped = backup::rotate(&backup_dir, cfg.backup.retain)?;
                    if dropped > 0 {
                        tracing::info!("rotated {} old backups", dropped);
                    }
                }
                Err(e) => tracing::warn!("backup failed (continuing): {e:#}"),
            }
        }

        let pool = ReaderPool::new(db_path.clone(), 4).await?;
        let fts = pool
            .with_conn(|c| fts::detect(c))
            .await
            .unwrap_or(None);
        match &fts {
            Some(cap) => tracing::info!(
                "FTS5 capability: detected (table={}, columns={:?})",
                cap.table,
                cap.columns
            ),
            None => tracing::info!("FTS5 capability: not detected; search uses LIKE fallback"),
        }
        Ok(Self {
            config: Arc::new(cfg),
            db_path,
            pool,
            test_db_mode,
            allow_writes_on_test_db: opts.allow_writes_on_test_db,
            fts,
        })
    }
}
