//! End-to-end exercise of `things_search` through the same surface the MCP
//! handler calls. Mirrors `end_to_end_plan_2.rs` — build a fixture DB, build
//! AppState pointed at it, and run the tool function against several
//! filter combinations.

use things_mcp::core::reader::fixture::build_fixture;
use things_mcp::state::{AppState, AppStateOptions};
use things_mcp::tools::lists::ProjectStatusArg;
use things_mcp::tools::search::{things_search, SearchArgs};

async fn build_state() -> AppState {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("things.sqlite");
    build_fixture(&db).unwrap();
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
    std::mem::forget(tmp);
    state
}

#[tokio::test]
async fn search_handles_text_and_structured_filters() {
    let state = build_state().await;

    // Text-only: open inbox + open anytime to-dos matching "milk".
    let by_text = things_search(
        state.clone(),
        SearchArgs {
            query: Some("milk".to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(by_text.iter().any(|t| t.title == "Buy milk"));

    // Status=All + completed text.
    let with_completed = things_search(
        state.clone(),
        SearchArgs {
            query: Some("tax".to_string()),
            status: Some(ProjectStatusArg::All),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(with_completed.iter().any(|t| t.title == "Pay tax bill"));

    // Tag filter (OR-semantic).
    let by_tag = things_search(
        state.clone(),
        SearchArgs {
            tags: vec!["Errand".to_string()],
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(by_tag.iter().any(|t| t.title == "Call the dentist"));

    // Area filter — todo-4 is in proj-1 (area-1).
    let by_area = things_search(
        state.clone(),
        SearchArgs {
            area_id: Some("area-1".to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let titles: Vec<_> = by_area.iter().map(|t| t.title.as_str()).collect();
    assert!(titles.contains(&"Read RFC 9457"));

    // Project filter.
    let by_project = things_search(
        state.clone(),
        SearchArgs {
            project_id: Some("proj-1".to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let titles: Vec<_> = by_project.iter().map(|t| t.title.as_str()).collect();
    assert!(titles.contains(&"Today scheduled item"));
    assert!(titles.contains(&"Read RFC 9457"));

    // Deadline range — only the 2099-dated row.
    let by_due = things_search(
        state.clone(),
        SearchArgs {
            due_after: Some("2050-01-01".to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let titles: Vec<_> = by_due.iter().map(|t| t.title.as_str()).collect();
    assert_eq!(titles, vec!["Upcoming deadlined item"]);

    // Scheduled range — only the 2020-dated row.
    let by_sched = things_search(
        state.clone(),
        SearchArgs {
            scheduled_before: Some("2050-01-01".to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let titles: Vec<_> = by_sched.iter().map(|t| t.title.as_str()).collect();
    assert!(titles.contains(&"Today scheduled item"));
    assert!(!titles.contains(&"Upcoming scheduled item"));

    // Combined text + area.
    let combined = things_search(
        state.clone(),
        SearchArgs {
            query: Some("Read".to_string()),
            area_id: Some("area-1".to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let titles: Vec<_> = combined.iter().map(|t| t.title.as_str()).collect();
    assert!(titles.contains(&"Read RFC 9457"));
    assert!(!titles.contains(&"Read research papers"));
}
