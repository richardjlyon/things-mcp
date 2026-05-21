use std::net::SocketAddr;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use rmcp::ServiceExt;
use things_mcp::{core::config, http_transport, logging, oauth, server::ThingsServer, setup, state};

#[derive(Parser)]
#[command(
    name = "things-mcp",
    about = "Local-first Things 3 bridge: runs as an MCP server over stdio (default), \
             HTTP (when THINGS_MCP_HTTP is set), or a setup helper for the HTTP \
             deployment."
)]
struct Cli {
    /// Override the live Things DB (test/dev use only).
    #[arg(long, value_name = "PATH")]
    db_path: Option<PathBuf>,
    /// Permit writes when --db-path overrides the live DB. Writes are dry-run.
    #[arg(long)]
    allow_writes_on_test_db: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Interactive setup: detect Tailscale, write launchd plist, enable
    /// Funnel, generate OAuth credentials, and print the values to paste into
    /// the Claude.ai connector. macOS-only.
    Setup,
    /// Read-only health check covering launchd, the HTTP server, Tailscale
    /// Funnel, and the OAuth config file.
    Status,
    /// Print the current OAuth client_id, client_secret, and connector URL
    /// from <config_dir>/oauth.toml in paste-ready form.
    ShowCredentials,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Setup) => setup::run_setup().await,
        Some(Command::Status) => setup::run_status().await,
        Some(Command::ShowCredentials) => setup::run_show_credentials(),
        None => run_server(cli.db_path, cli.allow_writes_on_test_db).await,
    }
}

async fn run_server(db_path: Option<PathBuf>, allow_writes_on_test_db: bool) -> anyhow::Result<()> {
    let env_db = db_path
        .or_else(|| std::env::var_os("THINGS_DB_PATH").map(PathBuf::from));
    let allow_writes = allow_writes_on_test_db
        || std::env::var("THINGS_MCP_ALLOW_WRITES_ON_TEST_DB")
            .ok()
            .as_deref()
            == Some("1");

    logging::init("info", None)?;
    tracing::info!("things-mcp {} starting (stdio or http)", env!("CARGO_PKG_VERSION"));

    let home = directories::UserDirs::new()
        .ok_or_else(|| anyhow::anyhow!("could not resolve home directory"))?
        .home_dir()
        .to_path_buf();
    let cfg_path = config::config_path()?;

    let state = state::AppState::build(state::AppStateOptions {
        env_db_path: env_db,
        home_dir: home,
        config_path: cfg_path,
        allow_writes_on_test_db: allow_writes,
        executor_override: None,
        applescript_override: None,
    })
    .await?;

    if let Ok(bind) = std::env::var("THINGS_MCP_HTTP") {
        // Bearer token is optional. Some MCP clients probe discovery endpoints
        // without sending an Authorization header; a blanket bearer check
        // breaks that handshake. When unset, the HTTP server runs without auth
        // — security depends on the transport (e.g., a private Tailscale Funnel URL).
        let token = std::env::var("THINGS_MCP_BEARER_TOKEN")
            .ok()
            .filter(|s| !s.is_empty());
        // OAuth 2.1 client credentials. Load from <config_dir>/oauth.toml; if
        // missing, generate iff THINGS_MCP_OAUTH_ISSUER is set (one-time
        // bootstrap that writes the file with mode 0600). Otherwise, OAuth is
        // disabled — the server runs without an auth gate.
        let issuer_hint = std::env::var("THINGS_MCP_OAUTH_ISSUER")
            .ok()
            .filter(|s| !s.is_empty());
        let oauth_state = match oauth::OAuthConfig::load_or_generate(issuer_hint)? {
            Some(cfg) => Some(oauth::OAuthState::from_default_path(cfg)?),
            None => None,
        };
        let addr: SocketAddr = bind
            .parse()
            .map_err(|e| anyhow::anyhow!("THINGS_MCP_HTTP must be host:port, got {bind:?}: {e}"))?;
        http_transport::run(state, addr, token, oauth_state).await
    } else {
        let server = ThingsServer::new(state);
        let transport = (tokio::io::stdin(), tokio::io::stdout());
        let running = server.serve(transport).await?;
        running.waiting().await?;
        Ok(())
    }
}
