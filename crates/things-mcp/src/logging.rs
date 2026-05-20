//! Tracing setup for things-mcp.
//!
//! stderr is always wired; if `log_dir` is provided we also append to
//! `<log_dir>/stdio.log`. Safe to call exactly once per process.

use std::path::Path;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

pub fn init(level: &str, log_dir: Option<&Path>) -> anyhow::Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));

    let stderr_layer = fmt::layer().with_writer(std::io::stderr).with_ansi(false);
    let registry = tracing_subscriber::registry()
        .with(filter)
        .with(stderr_layer);

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
