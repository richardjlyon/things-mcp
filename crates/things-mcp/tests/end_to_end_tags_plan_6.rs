//! End-to-end exercise of every Plan-6 tag tool.
//!
//! Eight tests run in test-DB DryRun mode against the fixture: writes
//! short-circuit before either the executor (`RecordingExecutor`) or the
//! AppleScript driver (`RecordingAppleScript`) is called, and the tools
//! return `dry_run: true`.
//!
//! The ninth test runs in Live mode with `RecordingAppleScript` injected,
//! and asserts the recorded script string equals what
//! `render_rename_tag(old, new)` produces — proving the
//! `applescript_override` seam delivers the rendered script intact.

use std::sync::Arc;

use things_mcp::core::applescript::driver::{AppleScriptDriver, RecordingAppleScript};
use things_mcp::core::applescript::script::render_rename_tag;
use things_mcp::core::reader::fixture::build_fixture;
use things_mcp::core::writer::executor::{Executor, RecordingExecutor};
use things_mcp::state::{AppState, AppStateOptions};
use things_mcp::tools::tags::{
    things_create_tag, things_delete_tag, things_list_tags, things_merge_tags,
    things_move_tag, things_rename_tag, CreateTagArgs, DeleteTagArgs, ListTagsArgs,
    MergeTagsArgs, MoveTagArgs, RenameTagArgs,
};
use things_mcp::tools::todos::{
    things_assign_tag, things_unassign_tag, TagAssignmentArgs,
};

/// Build an `AppState` in DryRun mode against the fixture, with both a
/// recording executor and a recording AppleScript driver injected. Returns
/// the state plus both recorders so tests can assert what was (or wasn't)
/// captured.
async fn build_dryrun_state() -> (
    AppState,
    Arc<RecordingExecutor>,
    Arc<RecordingAppleScript>,
) {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("things.sqlite");
    build_fixture(&db).unwrap();
    let recorder = Arc::new(RecordingExecutor::new());
    let applescript = Arc::new(RecordingAppleScript::new());
    let state = AppState::build(AppStateOptions {
        env_db_path: Some(db.clone()),
        home_dir: tmp.path().to_path_buf(),
        config_path: tmp.path().join("config.toml"),
        allow_writes_on_test_db: true,
        executor_override: Some(recorder.clone() as Arc<dyn Executor>),
        applescript_override: Some(applescript.clone() as Arc<dyn AppleScriptDriver>),
    })
    .await
    .unwrap();
    std::mem::forget(tmp);
    (state, recorder, applescript)
}

#[tokio::test]
async fn list_tags_returns_flat_and_roots_from_fixture() {
    let (state, _executor, _applescript) = build_dryrun_state().await;
    let listing = things_list_tags(state, ListTagsArgs::default()).await.unwrap();
    // Flat: 3 tags from the fixture.
    assert_eq!(listing.flat.len(), 3);
    // Roots: Errand + Deep work (Call has parent Errand).
    assert_eq!(listing.roots.len(), 2);
    let errand = listing.roots.iter().find(|r| r.title == "Errand").unwrap();
    assert_eq!(errand.children.len(), 1);
    assert_eq!(errand.children[0].title, "Call");
}

#[tokio::test]
async fn assign_tag_dry_run_does_not_call_executor() {
    let (state, executor, _applescript) = build_dryrun_state().await;
    let out = things_assign_tag(
        state,
        TagAssignmentArgs {
            id: "todo-1".into(),
            tags: vec!["Errand".into()],
        },
    )
    .await
    .unwrap();
    assert!(out.dry_run);
    assert_eq!(out.action, "update_todo");
    assert!(executor.urls().is_empty());
}

#[tokio::test]
async fn unassign_tag_dry_run_does_not_call_executor() {
    let (state, executor, _applescript) = build_dryrun_state().await;
    // todo-2 is tagged 'Errand' in the fixture.
    let out = things_unassign_tag(
        state,
        TagAssignmentArgs {
            id: "todo-2".into(),
            tags: vec!["Errand".into()],
        },
    )
    .await
    .unwrap();
    assert!(out.dry_run);
    assert_eq!(out.action, "update_todo");
    assert!(executor.urls().is_empty());
}

#[tokio::test]
async fn create_tag_dry_run_does_not_call_applescript_driver() {
    let (state, _executor, applescript) = build_dryrun_state().await;
    let out = things_create_tag(
        state,
        CreateTagArgs {
            name: "NewTag".into(),
            parent: None,
        },
    )
    .await
    .unwrap();
    assert!(out.dry_run);
    assert_eq!(out.action, "create_tag");
    assert!(applescript.scripts().is_empty());
}

#[tokio::test]
async fn rename_tag_dry_run_does_not_call_applescript_driver() {
    let (state, _executor, applescript) = build_dryrun_state().await;
    let out = things_rename_tag(
        state,
        RenameTagArgs {
            old: "Errand".into(),
            new: "Errands".into(),
        },
    )
    .await
    .unwrap();
    assert!(out.dry_run);
    assert_eq!(out.action, "rename_tag");
    assert!(applescript.scripts().is_empty());
}

#[tokio::test]
async fn merge_tags_dry_run_does_not_call_applescript_driver() {
    let (state, _executor, applescript) = build_dryrun_state().await;
    let out = things_merge_tags(
        state,
        MergeTagsArgs {
            source: "Errand".into(),
            target: "Deep work".into(),
        },
    )
    .await
    .unwrap();
    assert!(out.dry_run);
    assert_eq!(out.action, "merge_tags");
    assert!(applescript.scripts().is_empty());
}

#[tokio::test]
async fn delete_tag_dry_run_does_not_call_applescript_driver() {
    let (state, _executor, applescript) = build_dryrun_state().await;
    let out = things_delete_tag(
        state,
        DeleteTagArgs {
            name: "Errand".into(),
        },
    )
    .await
    .unwrap();
    assert!(out.dry_run);
    assert_eq!(out.action, "delete_tag");
    assert!(applescript.scripts().is_empty());
}

#[tokio::test]
async fn move_tag_dry_run_does_not_call_applescript_driver() {
    let (state, _executor, applescript) = build_dryrun_state().await;
    let out = things_move_tag(
        state,
        MoveTagArgs {
            name: "Call".into(),
            new_parent: None,
        },
    )
    .await
    .unwrap();
    assert!(out.dry_run);
    assert_eq!(out.action, "move_tag");
    assert!(applescript.scripts().is_empty());
}

#[tokio::test]
async fn rename_tag_live_mode_hands_rendered_script_to_recording_driver() {
    // Live mode: no `env_db_path` (so safety = Live). We still feed it a
    // fixture DB via config.toml so we don't touch the user's Things, and
    // we override the AppleScript driver with a recorder so no `osascript`
    // is actually spawned. The test asserts the script we recorded equals
    // what `render_rename_tag("Errand", "Errands")` produces.
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

[writer]
poll_timeout_ms = 100
poll_interval_ms = 10
"#,
            db.display(),
        ),
    )
    .unwrap();

    let applescript = Arc::new(RecordingAppleScript::new());
    let state = AppState::build(AppStateOptions {
        env_db_path: None,                 // Live mode
        home_dir: tmp.path().to_path_buf(),
        config_path: config_toml,
        allow_writes_on_test_db: false,
        executor_override: None,
        applescript_override: Some(applescript.clone() as Arc<dyn AppleScriptDriver>),
    })
    .await
    .unwrap();
    std::mem::forget(tmp);

    let out = things_rename_tag(
        state,
        RenameTagArgs {
            old: "Errand".into(),
            new: "Errands".into(),
        },
    )
    .await
    .unwrap();

    // Live mode → dry_run is false. The recorded script must equal what
    // the pure renderer produces.
    assert!(!out.dry_run);
    assert_eq!(out.action, "rename_tag");
    let scripts = applescript.scripts();
    assert_eq!(scripts.len(), 1);
    assert_eq!(scripts[0], render_rename_tag("Errand", "Errands"));
}

#[tokio::test]
async fn assign_tag_rejects_empty_element_in_tags_vec() {
    let (state, _executor, _applescript) = build_dryrun_state().await;
    let res = things_assign_tag(
        state,
        TagAssignmentArgs {
            id: "todo-1".into(),
            tags: vec!["Errand".into(), "".into()],
        },
    )
    .await;
    let err = res.expect_err("expected InvalidInput on empty tag element");
    let msg = format!("{err:#}");
    assert!(msg.contains("tags"), "error should mention the field: {msg}");
    assert!(
        msg.contains("empty") || msg.contains("whitespace"),
        "error should explain the cause: {msg}"
    );
}

#[tokio::test]
async fn unassign_tag_rejects_whitespace_only_element_in_tags_vec() {
    let (state, _executor, _applescript) = build_dryrun_state().await;
    let res = things_unassign_tag(
        state,
        TagAssignmentArgs {
            id: "todo-2".into(),
            tags: vec!["   ".into()],
        },
    )
    .await;
    let err = res.expect_err("expected InvalidInput on whitespace-only tag");
    let msg = format!("{err:#}");
    assert!(msg.contains("tags"), "error should mention the field: {msg}");
}
