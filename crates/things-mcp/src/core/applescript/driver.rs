//! AppleScript driver seam.
//!
//! Production: `OsascriptDriver` spawns `/usr/bin/osascript -e <script>`.
//! Tests: `RecordingAppleScript` captures every script string and pops queued
//! responses, so unit tests can assert exactly which AppleScript was emitted
//! without ever spawning `osascript`.

use std::collections::VecDeque;
use std::sync::Mutex;

use async_trait::async_trait;

use crate::core::error::ThingsError;

#[async_trait]
pub trait AppleScriptDriver: Send + Sync + std::fmt::Debug {
    /// Run the given AppleScript source. Returns stdout on success; returns
    /// `ThingsError::AppleScriptFailed { stderr, exit }` on non-zero exit.
    async fn run(&self, script: &str) -> Result<String, ThingsError>;
}

/// Production driver: shells out to `/usr/bin/osascript -e <script>`.
///
/// Things-not-running: a `tell application "Things3"` block in the rendered
/// script transparently launches Things on first call, so no explicit "is
/// Things running" probe is needed here. The startup `schema_probe` already
/// covers DB-side health.
#[derive(Debug, Default)]
pub struct OsascriptDriver;

#[async_trait]
impl AppleScriptDriver for OsascriptDriver {
    async fn run(&self, script: &str) -> Result<String, ThingsError> {
        let output = tokio::process::Command::new("/usr/bin/osascript")
            .arg("-e")
            .arg(script)
            .output()
            .await
            .map_err(|e| ThingsError::ExecutorFailed {
                message: format!("spawn /usr/bin/osascript: {e}"),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            let exit = output.status.code().unwrap_or(-1);
            return Err(ThingsError::AppleScriptFailed { stderr, exit });
        }

        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

/// Test driver: records every script it's asked to run without spawning
/// `osascript`. Tests assert on `scripts()` and seed `push_response()` to
/// control return values.
#[derive(Debug, Default)]
pub struct RecordingAppleScript {
    scripts: Mutex<Vec<String>>,
    responses: Mutex<VecDeque<Result<String, ThingsError>>>,
}

impl RecordingAppleScript {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns every script that has been passed to `run()`, in call order.
    pub fn scripts(&self) -> Vec<String> {
        self.scripts.lock().unwrap().clone()
    }

    /// Queue a response that the *next* call to `run()` will return. If no
    /// response has been queued, `run()` returns `Ok(String::new())`.
    pub fn push_response(&self, response: Result<String, ThingsError>) {
        self.responses.lock().unwrap().push_back(response);
    }
}

#[async_trait]
impl AppleScriptDriver for RecordingAppleScript {
    async fn run(&self, script: &str) -> Result<String, ThingsError> {
        self.scripts.lock().unwrap().push(script.to_string());
        match self.responses.lock().unwrap().pop_front() {
            Some(r) => r,
            None => Ok(String::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn recording_driver_captures_scripts_in_order() {
        let rec = RecordingAppleScript::new();
        rec.run("tell application \"Things3\" to make tag with properties {name:\"Work\"}")
            .await
            .unwrap();
        rec.run("tell application \"Things3\" to delete tag \"Old\"")
            .await
            .unwrap();
        let scripts = rec.scripts();
        assert_eq!(scripts.len(), 2);
        assert!(scripts[0].contains("make tag"));
        assert!(scripts[1].contains("delete tag"));
    }

    #[tokio::test]
    async fn recording_driver_replays_queued_responses_in_order() {
        let rec = RecordingAppleScript::new();
        rec.push_response(Ok("first".into()));
        rec.push_response(Err(ThingsError::AppleScriptFailed {
            stderr: "boom".into(),
            exit: 1,
        }));
        let r1 = rec.run("a").await.unwrap();
        assert_eq!(r1, "first");
        let r2 = rec.run("b").await;
        assert!(matches!(r2, Err(ThingsError::AppleScriptFailed { exit: 1, .. })));
        // Queue is now empty — the next call gets the default Ok(String::new()).
        let r3 = rec.run("c").await.unwrap();
        assert_eq!(r3, "");
    }

    // Manual smoke test: opt-in only — fires `/usr/bin/osascript` against
    // the local machine. Run with `cargo test -- --ignored
    // osascript_driver_smoke` only when you intend to.
    #[tokio::test]
    #[ignore = "fires /usr/bin/osascript on the local machine"]
    async fn osascript_driver_smoke() {
        let driver = OsascriptDriver;
        // Trivial script that returns "hello" — does NOT talk to Things.
        let out = driver
            .run("return \"hello\"")
            .await
            .expect("osascript should run");
        assert!(out.contains("hello"));
    }
}
