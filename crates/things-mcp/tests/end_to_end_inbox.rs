//! End-to-end exercise of the read pipeline: build a fixture DB, build
//! AppState pointed at it, call the tool function the MCP server delegates to,
//! assert the returned shape.
//!
//! This deliberately tests the library API (one rung below the MCP transport
//! layer) — the MCP wiring is a thin shim verified manually via Claude Code.

use things_mcp::core::reader::fixture::build_fixture;
use things_mcp::state::{AppState, AppStateOptions};
use things_mcp::tools::lists::{things_list_inbox, ListInboxArgs};

#[tokio::test]
async fn lists_inbox_against_fixture() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("things.sqlite");
    build_fixture(&db).unwrap();

    // home_dir + config_path are unused when env_db_path is set, but build()
    // still expects them to be present.
    let state = AppState::build(AppStateOptions {
        env_db_path: Some(db.clone()),
        home_dir: tmp.path().to_path_buf(),
        config_path: tmp.path().join("config.toml"),
        allow_writes_on_test_db: false,
        executor_override: None,
        applescript_override: None,
    })
    .await
    .unwrap();
    assert!(state.test_db_mode);

    let rows = things_list_inbox(state.clone(), ListInboxArgs::default())
        .await
        .unwrap();
    let titles: Vec<_> = rows.iter().map(|r| r.title.as_str()).collect();
    assert_eq!(rows.len(), 2);
    assert!(titles.contains(&"Buy milk"));
    assert!(titles.contains(&"Call the dentist"));

    let with_completed = things_list_inbox(
        state,
        ListInboxArgs {
            limit: None,
            include_completed: Some(true),
        },
    )
    .await
    .unwrap();
    assert_eq!(with_completed.len(), 3);
}
