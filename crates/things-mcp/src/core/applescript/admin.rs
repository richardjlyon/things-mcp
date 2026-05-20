//! `TagAdmin` — facade over the AppleScript driver. Owns the safety gate
//! and composes a `TagOutcome` per call. Each method renders the script
//! via the pure helpers in `script.rs`, then either short-circuits
//! (DryRun), errors out (Forbidden), or hands the script to the injected
//! `AppleScriptDriver`.

use std::sync::Arc;
use std::time::Instant;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::core::applescript::driver::AppleScriptDriver;
use crate::core::applescript::script;
use crate::core::error::ThingsError;
use crate::core::writer::writer::SafetyMode;

#[derive(Debug)]
pub struct TagAdmin {
    pub driver: Arc<dyn AppleScriptDriver>,
    pub safety: SafetyMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TagOutcome {
    /// Snake-case action name — `"create_tag"`, `"rename_tag"`, …
    pub action: String,
    /// `true` when the safety gate short-circuited (DryRun mode). When
    /// `true`, the script was rendered but never run; `osascript_stdout`
    /// is empty.
    pub dry_run: bool,
    /// Wall-clock latency in milliseconds. `0` when `dry_run` is true.
    pub latency_ms: u64,
    /// First line of `osascript` stdout, truncated to 200 chars. Empty when
    /// the script returned no output (the common case for tag-admin ops).
    pub osascript_stdout: String,
}

impl TagAdmin {
    pub async fn create(&self, name: &str, parent: Option<&str>) -> Result<TagOutcome, ThingsError> {
        let script = script::render_create_tag(name, parent);
        self.dispatch("create_tag", script).await
    }

    pub async fn rename(&self, old: &str, new: &str) -> Result<TagOutcome, ThingsError> {
        let script = script::render_rename_tag(old, new);
        self.dispatch("rename_tag", script).await
    }

    pub async fn merge(&self, source: &str, target: &str) -> Result<TagOutcome, ThingsError> {
        // Defense-in-depth: the tool adapter also rejects this, but a stray
        // direct caller would render a script that deletes the only copy
        // and then tries to read from it.
        if source == target {
            return Err(ThingsError::InvalidInput {
                field: "source".into(),
                reason: "source and target must differ".into(),
            });
        }
        let script = script::render_merge_tags(source, target);
        self.dispatch("merge_tags", script).await
    }

    pub async fn delete(&self, name: &str) -> Result<TagOutcome, ThingsError> {
        let script = script::render_delete_tag(name);
        self.dispatch("delete_tag", script).await
    }

    pub async fn move_under(
        &self,
        name: &str,
        new_parent: Option<&str>,
    ) -> Result<TagOutcome, ThingsError> {
        let script = script::render_move_tag(name, new_parent);
        self.dispatch("move_tag", script).await
    }

    async fn dispatch(&self, action: &str, script: String) -> Result<TagOutcome, ThingsError> {
        // 1. Safety gate — Forbidden refuses outright.
        if self.safety == SafetyMode::Forbidden {
            return Err(ThingsError::TestDbWriteForbidden);
        }

        // 2. Log the script (no secrets to mask — AppleScript doesn't carry
        // the auth-token).
        tracing::info!(action = action, "applescript: {} bytes", script.len());

        // 3. DryRun short-circuit — render only, no driver call.
        if self.safety == SafetyMode::DryRun {
            return Ok(TagOutcome {
                action: action.to_string(),
                dry_run: true,
                latency_ms: 0,
                osascript_stdout: String::new(),
            });
        }

        // 4. Live: hand the script to the driver.
        let started = Instant::now();
        let stdout = self.driver.run(&script).await?;
        let latency_ms = started.elapsed().as_millis() as u64;
        let truncated = truncate_first_line(&stdout, 200);

        Ok(TagOutcome {
            action: action.to_string(),
            dry_run: false,
            latency_ms,
            osascript_stdout: truncated,
        })
    }
}

fn truncate_first_line(s: &str, max: usize) -> String {
    let first = s.lines().next().unwrap_or("");
    if first.len() <= max {
        first.to_string()
    } else {
        first.chars().take(max).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::applescript::driver::RecordingAppleScript;

    fn admin(safety: SafetyMode) -> (Arc<RecordingAppleScript>, TagAdmin) {
        let rec = Arc::new(RecordingAppleScript::new());
        let admin = TagAdmin {
            driver: rec.clone(),
            safety,
        };
        (rec, admin)
    }

    #[tokio::test]
    async fn forbidden_mode_refuses_outright() {
        let (rec, admin) = admin(SafetyMode::Forbidden);
        let res = admin.create("Work", None).await;
        assert!(matches!(res, Err(ThingsError::TestDbWriteForbidden)));
        // Driver must NOT have been called.
        assert!(rec.scripts().is_empty());
    }

    #[tokio::test]
    async fn dry_run_mode_short_circuits_without_calling_driver() {
        let (rec, admin) = admin(SafetyMode::DryRun);
        let out = admin.create("Work", None).await.unwrap();
        assert!(out.dry_run);
        assert_eq!(out.action, "create_tag");
        assert_eq!(out.latency_ms, 0);
        assert_eq!(out.osascript_stdout, "");
        // Driver was never invoked.
        assert!(rec.scripts().is_empty());
    }

    #[tokio::test]
    async fn live_create_calls_driver_with_rendered_script() {
        let (rec, admin) = admin(SafetyMode::Live);
        let out = admin.create("Work", Some("Personal")).await.unwrap();
        assert!(!out.dry_run);
        assert_eq!(out.action, "create_tag");
        let scripts = rec.scripts();
        assert_eq!(scripts.len(), 1);
        assert!(scripts[0].contains("make new tag with properties {name:\"Work\"}"));
        assert!(scripts[0].contains("set parent tag of newTag to tag \"Personal\""));
    }

    #[tokio::test]
    async fn live_rename_calls_driver_with_rendered_script() {
        let (rec, admin) = admin(SafetyMode::Live);
        let _out = admin.rename("Old", "New").await.unwrap();
        let scripts = rec.scripts();
        assert!(scripts[0].contains("set name of tag \"Old\" to \"New\""));
    }

    #[tokio::test]
    async fn live_merge_calls_driver_with_rendered_script() {
        let (rec, admin) = admin(SafetyMode::Live);
        let _out = admin.merge("Source", "Target").await.unwrap();
        let scripts = rec.scripts();
        assert!(scripts[0].contains("set sourceTag to tag \"Source\""));
        assert!(scripts[0].contains("delete sourceTag"));
    }

    #[tokio::test]
    async fn merge_self_rejected_with_invalid_input() {
        let (rec, admin) = admin(SafetyMode::Live);
        let res = admin.merge("Same", "Same").await;
        match res {
            Err(ThingsError::InvalidInput { field, .. }) => assert_eq!(field, "source"),
            other => panic!("expected InvalidInput, got {:?}", other),
        }
        // Driver must not have been called for the self-merge.
        assert!(rec.scripts().is_empty());
    }

    #[tokio::test]
    async fn live_delete_calls_driver_with_rendered_script() {
        let (rec, admin) = admin(SafetyMode::Live);
        let _out = admin.delete("Stale").await.unwrap();
        let scripts = rec.scripts();
        assert!(scripts[0].contains("delete tag \"Stale\""));
    }

    #[tokio::test]
    async fn live_move_under_parent_calls_driver_with_rendered_script() {
        let (rec, admin) = admin(SafetyMode::Live);
        let _out = admin.move_under("Urgent", Some("Work")).await.unwrap();
        let scripts = rec.scripts();
        assert!(scripts[0].contains("set parent tag of tag \"Urgent\" to tag \"Work\""));
    }

    #[tokio::test]
    async fn live_move_to_root_uses_missing_value() {
        let (rec, admin) = admin(SafetyMode::Live);
        let _out = admin.move_under("Urgent", None).await.unwrap();
        let scripts = rec.scripts();
        assert!(scripts[0].contains("set parent tag of tag \"Urgent\" to missing value"));
    }

    #[tokio::test]
    async fn driver_error_propagates_unchanged() {
        let (rec, admin) = admin(SafetyMode::Live);
        rec.push_response(Err(ThingsError::AppleScriptFailed {
            stderr: "tag not found".into(),
            exit: 1,
        }));
        let res = admin.delete("Ghost").await;
        match res {
            Err(ThingsError::AppleScriptFailed { stderr, exit }) => {
                assert_eq!(stderr, "tag not found");
                assert_eq!(exit, 1);
            }
            other => panic!("expected AppleScriptFailed, got {:?}", other),
        }
    }
}
