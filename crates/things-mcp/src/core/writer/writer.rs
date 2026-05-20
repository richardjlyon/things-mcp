//! `Writer` — the keystone of `core/writer/`. Glues operation rendering,
//! URL composition, the executor seam, and post-write verification together
//! behind one method, `fire()`. Safety gates enforced up front: writes are
//! refused in test-DB mode unless explicitly opted in, and creates short-
//! circuit to a dry-run outcome in that mode without ever firing the
//! executor.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::core::error::ThingsError;
use crate::core::reader::pool::ReaderPool;
use crate::core::writer::executor::Executor;
use crate::core::writer::operation::Operation;
use crate::core::writer::outcome::WriteOutcome;
use crate::core::writer::secret::SecretString;
use crate::core::writer::url::{build_url, mask_auth_token};
use crate::core::writer::verify::{verify, VerifyOutcome, VerifyPredicate};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SafetyMode {
    /// Live: writes fire normally.
    Live,
    /// Test-DB mode with the explicit opt-in: build the URL, log it, do
    /// NOT call the executor. `WriteOutcome { dry_run: true }`.
    DryRun,
    /// Test-DB mode without the opt-in: refuse with `TestDbWriteForbidden`.
    Forbidden,
}

#[derive(Debug, Clone, Copy)]
pub struct WriterCfg {
    pub poll_timeout: Duration,
    pub poll_interval: Duration,
}

#[derive(Debug, Clone)]
pub struct Writer {
    pub executor: Arc<dyn Executor>,
    pub pool: ReaderPool,
    pub auth: Option<SecretString>,
    pub cfg: WriterCfg,
    pub safety: SafetyMode,
}

impl Writer {
    pub async fn fire(
        &self,
        op: Operation,
        verify_pred: Option<VerifyPredicate>,
    ) -> Result<WriteOutcome, ThingsError> {
        // 1. Safety gate — refuse outright before doing any work.
        if self.safety == SafetyMode::Forbidden {
            return Err(ThingsError::TestDbWriteForbidden);
        }

        // 2. Auth gate — only operations that require the token care, and only
        // in Live mode (DryRun never calls the executor so the token is
        // unnecessary there).
        if op.requires_auth_token() && self.auth.is_none() && self.safety == SafetyMode::Live {
            return Err(ThingsError::MissingAuthToken {
                hint: "set THINGS_AUTH_TOKEN or config.toml [things].auth_token".into(),
            });
        }

        // 3. Build URL.
        let url = build_url(&[op.clone()], self.auth.as_ref());

        // 4. Log URL (masked).
        tracing::info!(action = op.action_name(), "write: {}", mask_auth_token(&url));

        // 5. Dry-run short-circuit.
        if self.safety == SafetyMode::DryRun {
            return Ok(WriteOutcome {
                id: None,
                action: op.action_name().to_string(),
                verified: false,
                dry_run: true,
                latency_ms: 0,
            });
        }

        // 6. Open URL via the injected executor.
        let started = Instant::now();
        self.executor.open(&url).await?;

        // 7. Verify by polling the reader (or skip if None).
        let Some(pred) = verify_pred else {
            // No verify predicate → bulk path. Return success-with-verified=false
            // immediately after the executor call.
            let latency_ms = started.elapsed().as_millis() as u64;
            return Ok(WriteOutcome {
                id: None,
                action: op.action_name().to_string(),
                verified: false,
                dry_run: false,
                latency_ms,
            });
        };

        let outcome = verify(
            &self.pool,
            pred,
            self.cfg.poll_timeout,
            self.cfg.poll_interval,
        )
        .await?;

        // 8. Compose outcome.
        let latency_ms = started.elapsed().as_millis() as u64;
        Ok(match outcome {
            VerifyOutcome::Verified { row, .. } => WriteOutcome {
                id: Some(row.id),
                action: op.action_name().to_string(),
                verified: true,
                dry_run: false,
                latency_ms,
            },
            VerifyOutcome::Timeout { .. } | VerifyOutcome::NotFound { .. } => WriteOutcome {
                id: None,
                action: op.action_name().to_string(),
                verified: false,
                dry_run: false,
                latency_ms,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reader::fixture::build_fixture;
    use crate::core::writer::executor::RecordingExecutor;
    use crate::core::writer::operation::AddTodoSpec;
    use tempfile::tempdir;

    async fn build_writer(safety: SafetyMode) -> (tempfile::TempDir, Writer, Arc<RecordingExecutor>) {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("p.sqlite");
        build_fixture(&path).unwrap();
        let pool = ReaderPool::new(path, 2).await.unwrap();
        let exec = Arc::new(RecordingExecutor::new());
        let writer = Writer {
            executor: exec.clone(),
            pool,
            auth: None,
            cfg: WriterCfg {
                poll_timeout: Duration::from_millis(200),
                poll_interval: Duration::from_millis(20),
            },
            safety,
        };
        (tmp, writer, exec)
    }

    fn add_op(title: &str) -> Operation {
        Operation::AddTodo(AddTodoSpec {
            title: title.into(),
            ..Default::default()
        })
    }

    fn pred(title: &str) -> VerifyPredicate {
        use crate::core::types::TaskKind;
        VerifyPredicate::CreateByTitle {
            title: title.into(),
            since_unix: 0.0,
            kind: TaskKind::Todo,
        }
    }

    #[tokio::test]
    async fn fire_returns_test_db_write_forbidden_in_forbidden_mode() {
        let (_tmp, writer, exec) = build_writer(SafetyMode::Forbidden).await;
        let res = writer.fire(add_op("anything"), Some(pred("anything"))).await;
        assert!(matches!(res, Err(ThingsError::TestDbWriteForbidden)));
        // Executor must NOT have been called.
        assert!(exec.urls().is_empty());
    }

    #[tokio::test]
    async fn fire_dry_run_short_circuits_without_calling_executor() {
        let (_tmp, writer, exec) = build_writer(SafetyMode::DryRun).await;
        let out = writer
            .fire(add_op("Pretend to buy bread"), Some(pred("Pretend to buy bread")))
            .await
            .unwrap();
        assert!(out.dry_run);
        assert!(!out.verified);
        assert_eq!(out.action, "add_todo");
        assert_eq!(out.latency_ms, 0);
        // Executor must NOT have been called in dry-run.
        assert!(exec.urls().is_empty());
    }

    #[tokio::test]
    async fn fire_live_calls_executor_then_times_out_against_test_db() {
        // In a test-fixture DB with no Things app behind it, verify will time out.
        // This is the happy "executor-was-called-but-no-row-appeared" path,
        // which lets us assert the URL was emitted AND the timeout outcome.
        let (_tmp, writer, exec) = build_writer(SafetyMode::Live).await;
        let out = writer
            .fire(
                add_op("Definitely-not-in-fixture row"),
                Some(pred("Definitely-not-in-fixture row")),
            )
            .await
            .unwrap();
        // Executor was called exactly once.
        let urls = exec.urls();
        assert_eq!(urls.len(), 1);
        assert!(urls[0].starts_with("things:///json?data="));
        // Verify timed out because the fixture has no such row.
        assert!(!out.dry_run);
        assert!(!out.verified);
        assert_eq!(out.action, "add_todo");
        assert!(out.latency_ms >= 200, "should reach the configured timeout");
    }

    #[tokio::test]
    async fn fire_with_none_verify_pred_skips_verify_and_returns_unverified() {
        let (tmp, base_writer, exec) = build_writer(SafetyMode::Live).await;
        // BulkRaw conservatively requires_auth_token=true; supply a dummy token
        // so the auth gate doesn't fire before we reach the executor.
        let writer = Writer {
            auth: Some(SecretString::new("dummy-token-for-test")),
            ..base_writer
        };
        let _tmp = tmp; // keep tempdir alive
        let bulk_op = Operation::BulkRaw(crate::core::writer::operation::BulkRawSpec {
            operations: vec![serde_json::json!({
                "type": "to-do",
                "attributes": {"title": "Anything"}
            })],
        });
        let started = std::time::Instant::now();
        let out = writer.fire(bulk_op, None).await.unwrap();
        // Executor called once.
        let urls = exec.urls();
        assert_eq!(urls.len(), 1);
        // No verify polling — should return well before the configured 200ms timeout.
        assert!(
            started.elapsed() < std::time::Duration::from_millis(150),
            "fire(None) must skip verify and return promptly; elapsed: {:?}",
            started.elapsed()
        );
        // Outcome: unverified (no predicate to verify against), not dry-run.
        assert!(!out.verified);
        assert!(!out.dry_run);
        assert_eq!(out.action, "bulk_json");
        assert!(out.id.is_none());
    }
}
