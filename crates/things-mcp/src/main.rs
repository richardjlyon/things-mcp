use things_mcp::logging;

fn main() -> anyhow::Result<()> {
    logging::init("info", None)?;
    tracing::info!("things-mcp {} starting", env!("CARGO_PKG_VERSION"));
    Ok(())
}
