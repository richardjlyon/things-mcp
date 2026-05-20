//! End-to-end exercise of every Plan-5 write tool in dry-run mode. Each test
//! constructs an AppState with the test-DB safety gate set to DryRun and a
//! RecordingExecutor injected. Because dry-run short-circuits before the
//! executor is called, every test asserts `recorder.urls().is_empty()` plus
//! `out.dry_run == true`.
//!
//! ONE exception: the `update_todo_includes_auth_token_when_configured` test
//! switches to Live mode and asserts the recorded URL contains the
//! percent-encoded auth-token. This is the first end-to-end exercise of the
//! auth path.

use std::sync::Arc;

use things_mcp::core::reader::fixture::build_fixture;
use things_mcp::core::writer::executor::{Executor, RecordingExecutor};
use things_mcp::state::{AppState, AppStateOptions};
use things_mcp::tools::bulk::{things_bulk_json, BulkJsonArgs};
use things_mcp::tools::projects::{
    things_add_project, things_update_project, AddProjectArgs, UpdateProjectArgs,
};
use things_mcp::tools::todos::{
    things_cancel_todo, things_complete_todo, things_move_todo, things_update_todo,
    MoveTodoArgs, StatusChangeArgs, UpdateTodoArgs,
};

async fn build_dryrun_state(
    recorder: Arc<dyn Executor>,
) -> AppState {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("things.sqlite");
    build_fixture(&db).unwrap();
    let state = AppState::build(AppStateOptions {
        env_db_path: Some(db.clone()),
        home_dir: tmp.path().to_path_buf(),
        config_path: tmp.path().join("config.toml"),
        allow_writes_on_test_db: true,
        executor_override: Some(recorder),
    })
    .await
    .unwrap();
    std::mem::forget(tmp);
    state
}

#[tokio::test]
async fn add_project_dry_run_does_not_call_executor() {
    let recorder = Arc::new(RecordingExecutor::new());
    let state = build_dryrun_state(recorder.clone() as Arc<dyn Executor>).await;
    let out = things_add_project(
        state,
        AddProjectArgs {
            title: "Launch website".into(),
            area_id: Some("area-2".into()),
            headings: vec!["Design".into(), "QA".into()],
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(out.dry_run);
    assert_eq!(out.action, "add_project");
    assert!(recorder.urls().is_empty());
}

#[tokio::test]
async fn update_todo_dry_run_does_not_call_executor() {
    let recorder = Arc::new(RecordingExecutor::new());
    let state = build_dryrun_state(recorder.clone() as Arc<dyn Executor>).await;
    let out = things_update_todo(
        state,
        UpdateTodoArgs {
            id: "todo-1".into(),
            title: Some("Buy oat milk".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(out.dry_run);
    assert_eq!(out.action, "update_todo");
    assert!(recorder.urls().is_empty());
}

#[tokio::test]
async fn update_project_dry_run_does_not_call_executor() {
    let recorder = Arc::new(RecordingExecutor::new());
    let state = build_dryrun_state(recorder.clone() as Arc<dyn Executor>).await;
    let out = things_update_project(
        state,
        UpdateProjectArgs {
            id: "proj-1".into(),
            title: Some("Reading list — 2026".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(out.dry_run);
    assert_eq!(out.action, "update_project");
    assert!(recorder.urls().is_empty());
}

#[tokio::test]
async fn complete_todo_dry_run_does_not_call_executor() {
    let recorder = Arc::new(RecordingExecutor::new());
    let state = build_dryrun_state(recorder.clone() as Arc<dyn Executor>).await;
    let out = things_complete_todo(
        state,
        StatusChangeArgs { id: "todo-1".into() },
    )
    .await
    .unwrap();
    assert!(out.dry_run);
    assert_eq!(out.action, "complete_todo");
    assert!(recorder.urls().is_empty());
}

#[tokio::test]
async fn cancel_todo_dry_run_does_not_call_executor() {
    let recorder = Arc::new(RecordingExecutor::new());
    let state = build_dryrun_state(recorder.clone() as Arc<dyn Executor>).await;
    let out = things_cancel_todo(
        state,
        StatusChangeArgs { id: "todo-1".into() },
    )
    .await
    .unwrap();
    assert!(out.dry_run);
    assert_eq!(out.action, "cancel_todo");
    assert!(recorder.urls().is_empty());
}

#[tokio::test]
async fn move_todo_dry_run_does_not_call_executor() {
    let recorder = Arc::new(RecordingExecutor::new());
    let state = build_dryrun_state(recorder.clone() as Arc<dyn Executor>).await;
    let out = things_move_todo(
        state,
        MoveTodoArgs {
            id: "todo-1".into(),
            list_id: Some("proj-1".into()),
        },
    )
    .await
    .unwrap();
    assert!(out.dry_run);
    assert_eq!(out.action, "move_todo");
    assert!(recorder.urls().is_empty());
}

#[tokio::test]
async fn bulk_json_dry_run_does_not_call_executor() {
    let recorder = Arc::new(RecordingExecutor::new());
    let state = build_dryrun_state(recorder.clone() as Arc<dyn Executor>).await;
    let out = things_bulk_json(
        state,
        BulkJsonArgs {
            operations: vec![
                serde_json::json!({
                    "type": "to-do",
                    "attributes": { "title": "Bulk A" }
                }),
                serde_json::json!({
                    "type": "to-do",
                    "attributes": { "title": "Bulk B" }
                }),
            ],
        },
    )
    .await
    .unwrap();
    assert!(out.dry_run);
    assert_eq!(out.action, "bulk_json");
    assert!(recorder.urls().is_empty());
}

#[tokio::test]
async fn update_todo_includes_auth_token_in_url_when_configured() {
    // End-to-end auth-token exercise. We need Live mode (so the dry-run gate
    // doesn't short-circuit before the executor) but with a fixture DB (so
    // we don't touch the user's real Things). Live mode is the default when
    // `env_db_path` is `None`; the fixture path is resolved out of
    // `cfg.things.db_path` from config.toml.
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("things.sqlite");
    build_fixture(&db).unwrap();
    let config_toml = tmp.path().join("config.toml");
    std::fs::write(
        &config_toml,
        format!(
            r#"
[things]
db_path = "{}"
auth_token = "test-token-123"

[writer]
poll_timeout_ms = 100
poll_interval_ms = 10
"#,
            db.display(),
        ),
    )
    .unwrap();
    let recorder = Arc::new(RecordingExecutor::new());
    let state = AppState::build(AppStateOptions {
        env_db_path: None,  // Live mode — auth-token is loaded from config.toml
        home_dir: tmp.path().to_path_buf(),
        config_path: config_toml,
        allow_writes_on_test_db: false,
        executor_override: Some(recorder.clone() as Arc<dyn Executor>),
    })
    .await
    .unwrap();
    std::mem::forget(tmp);

    let out = things_update_todo(
        state,
        UpdateTodoArgs {
            id: "todo-1".into(),
            title: Some("Buy oat milk".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // The fixture has no live Things app, so verify will time out. That's
    // fine — what we're proving is that the URL was constructed AND the
    // auth-token segment was included.
    assert!(!out.dry_run);
    assert!(!out.verified);
    assert_eq!(out.action, "update_todo");
    let urls = recorder.urls();
    assert_eq!(urls.len(), 1, "executor should record exactly one URL");
    // The literal token survives percent-encoding because it contains only
    // unreserved characters (alpha + digit + hyphen).
    assert!(
        urls[0].contains("&auth-token=test-token-123"),
        "URL should carry the percent-encoded auth-token; got: {}",
        urls[0],
    );
}
