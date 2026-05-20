//! End-to-end exercise of the Plan-2 read pipeline: build a fixture DB,
//! build AppState pointed at it, call each tool function the MCP server
//! delegates to, and assert the returned shape. Same approach as Plan 1's
//! `end_to_end_inbox.rs` — tests the library API one rung below MCP transport.

use things_mcp::core::reader::fixture::build_fixture;
use things_mcp::state::{AppState, AppStateOptions};
use things_mcp::tools::lists::{
    things_list_anytime, things_list_areas, things_list_by_tag, things_list_logbook,
    things_list_projects, things_list_someday, things_list_tags, things_list_today,
    things_list_trash, things_list_upcoming, ListAnytimeArgs, ListAreasArgs, ListByTagArgs,
    ListLogbookArgs, ListProjectsArgs, ListSomedayArgs, ListTagsArgs, ListTodayArgs,
    ListTrashArgs, ListUpcomingArgs,
};
use things_mcp::tools::projects::{things_get_project, GetProjectArgs};
use things_mcp::tools::todos::{things_get_todo, GetTodoArgs};

async fn build_state() -> AppState {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("things.sqlite");
    build_fixture(&db).unwrap();
    let state = AppState::build(AppStateOptions {
        env_db_path: Some(db.clone()),
        home_dir: tmp.path().to_path_buf(),
        config_path: tmp.path().join("config.toml"),
        allow_writes_on_test_db: false,
    })
    .await
    .unwrap();
    // Keep tmp alive by leaking it into the state's pool path; tempdir is dropped
    // when the test function returns, but the DB file remains via the pool until
    // the state is dropped. Tests are short-lived so this is fine.
    std::mem::forget(tmp);
    state
}

#[tokio::test]
async fn plan_2_surface_returns_expected_shapes() {
    let state = build_state().await;

    // Lists
    let today = things_list_today(state.clone(), ListTodayArgs::default()).await.unwrap();
    assert!(today.iter().any(|t| t.title == "Today scheduled item"));

    let upcoming = things_list_upcoming(state.clone(), ListUpcomingArgs::default()).await.unwrap();
    assert!(upcoming.iter().any(|t| t.title == "Upcoming scheduled item"));
    assert!(upcoming.iter().any(|t| t.title == "Upcoming deadlined item"));

    let anytime = things_list_anytime(state.clone(), ListAnytimeArgs::default()).await.unwrap();
    assert!(anytime.iter().any(|t| t.title == "Read RFC 9457"));

    let someday = things_list_someday(state.clone(), ListSomedayArgs::default()).await.unwrap();
    assert!(someday.iter().any(|t| t.title == "Read research papers"));

    let logbook = things_list_logbook(state.clone(), ListLogbookArgs::default()).await.unwrap();
    assert!(logbook.iter().any(|t| t.title == "Old completed"));
    assert!(logbook.iter().any(|t| t.title == "Old canceled"));

    let trash = things_list_trash(state.clone(), ListTrashArgs::default()).await.unwrap();
    assert!(trash.iter().any(|t| t.title == "Trashed thing"));

    // Areas + projects + tags
    let areas = things_list_areas(state.clone(), ListAreasArgs::default()).await.unwrap();
    assert_eq!(areas.len(), 2);

    let projects_open = things_list_projects(state.clone(), ListProjectsArgs::default()).await.unwrap();
    assert!(projects_open.iter().any(|p| p.title == "Reading list"));

    let tags = things_list_tags(state.clone(), ListTagsArgs::default()).await.unwrap();
    assert_eq!(tags.len(), 3);
    assert!(tags.iter().any(|t| t.title == "Call" && t.parent_id.as_deref() == Some("tag-errand")));

    // Single-entity reads
    let todo = things_get_todo(
        state.clone(),
        GetTodoArgs {
            id: "todo-1".to_string(),
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(todo.summary.title, "Buy milk");
    assert_eq!(todo.checklist.len(), 3);

    let project = things_get_project(
        state.clone(),
        GetProjectArgs {
            id: "proj-1".to_string(),
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(project.headings.len(), 1);
    assert_eq!(project.headings[0].title, "Articles");

    // Tag drill-down
    let by_tag = things_list_by_tag(
        state.clone(),
        ListByTagArgs {
            tag: "Errand".to_string(),
            recurse: Some(true),
            limit: None,
        },
    )
    .await
    .unwrap();
    let titles: Vec<_> = by_tag.iter().map(|t| t.title.as_str()).collect();
    assert!(titles.contains(&"Call the dentist"));
    assert!(titles.contains(&"Read RFC 9457"));
}
