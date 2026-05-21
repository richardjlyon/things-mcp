use std::path::PathBuf;

use clap::Parser;
use rmcp::ServiceExt;
use things_mcp::{
    core::config,
    logging,
    server::ThingsServer,
    state::{AppState, AppStateOptions},
};

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
    let env_db = cli
        .db_path
        .or_else(|| std::env::var_os("THINGS_DB_PATH").map(PathBuf::from));
    let allow_writes = cli.allow_writes_on_test_db
        || std::env::var("THINGS_MCP_ALLOW_WRITES_ON_TEST_DB")
            .ok()
            .as_deref()
            == Some("1");

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
        executor_override: None,
        applescript_override: None,
    })
    .await?;

    let server = ThingsServer::new(state);
    let transport = (tokio::io::stdin(), tokio::io::stdout());
    let running = server.serve(transport).await?;
    running.waiting().await?;
    Ok(())
}
