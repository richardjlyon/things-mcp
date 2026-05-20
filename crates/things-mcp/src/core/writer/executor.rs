//! Executor seam: how a built URL gets handed to macOS so Things can
//! process it. Production uses `OpenCommandExecutor` (spawns
//! `/usr/bin/open -g <url>`). Tests substitute `RecordingExecutor`,
//! which captures URLs without spawning anything.

use std::sync::Mutex;

use async_trait::async_trait;

use crate::core::error::ThingsError;

#[async_trait]
pub trait Executor: Send + Sync + std::fmt::Debug {
    /// Hand a `things://` URL to the platform. Returns once `/usr/bin/open`
    /// (or the test substitute) has been invoked — does NOT wait for the
    /// Things app to actually process it. Post-write verification is the
    /// `verify` module's job.
    async fn open(&self, url: &str) -> Result<(), ThingsError>;
}

/// Production executor: shells out to `/usr/bin/open -g <url>`.
/// The `-g` flag opens the URL in the background so Things doesn't yank
/// focus from whatever the user is doing.
#[derive(Debug, Default)]
pub struct OpenCommandExecutor;

#[async_trait]
impl Executor for OpenCommandExecutor {
    async fn open(&self, url: &str) -> Result<(), ThingsError> {
        let status = tokio::process::Command::new("/usr/bin/open")
            .arg("-g")
            .arg(url)
            .status()
            .await
            .map_err(|e| ThingsError::ExecutorFailed {
                message: format!("spawn /usr/bin/open: {e}"),
            })?;
        if !status.success() {
            return Err(ThingsError::ExecutorFailed {
                message: format!("/usr/bin/open exited {status}"),
            });
        }
        Ok(())
    }
}

/// Test executor: records every URL it's asked to open without spawning
/// anything. Use `urls()` to inspect what was captured.
#[derive(Debug, Default)]
pub struct RecordingExecutor {
    urls: Mutex<Vec<String>>,
}

impl RecordingExecutor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn urls(&self) -> Vec<String> {
        self.urls.lock().unwrap().clone()
    }
}

#[async_trait]
impl Executor for RecordingExecutor {
    async fn open(&self, url: &str) -> Result<(), ThingsError> {
        self.urls.lock().unwrap().push(url.to_string());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn recording_executor_captures_urls_in_order() {
        let rec = RecordingExecutor::new();
        rec.open("things:///json?data=%5B%5D").await.unwrap();
        rec.open("things:///json?data=%5Bx%5D").await.unwrap();
        let urls = rec.urls();
        assert_eq!(urls.len(), 2);
        assert!(urls[0].contains("%5B%5D"));
        assert!(urls[1].contains("%5Bx%5D"));
    }

    // Manual smoke test: opt-in only — fires `/usr/bin/open` against the
    // user's real Things app. Run with `cargo test -- --ignored
    // open_command_executor_smoke` only when you mean to.
    #[tokio::test]
    #[ignore = "fires /usr/bin/open against the real Things app"]
    async fn open_command_executor_smoke() {
        let exec = OpenCommandExecutor;
        exec.open("things:///")
            .await
            .expect("open should not fail");
    }
}
