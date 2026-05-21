//! Read tools that surface a Things list view.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::core::reader::queries::{list_areas, list_inbox, list_today, ListInboxParams, ListTodayParams};
use crate::core::types::{AreaList, ProjectList, TodoList};
use crate::core::reader::queries::{list_upcoming, ListUpcomingParams};
use crate::core::reader::queries::{list_anytime, ListAnytimeParams};
use crate::core::reader::queries::{list_someday, ListSomedayParams};
use crate::core::reader::queries::{list_logbook, ListLogbookParams};
use crate::core::reader::queries::{list_trash, ListTrashParams};
use crate::state::AppState;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct ListInboxArgs {
    /// Cap on returned rows. Defaults to 200.
    #[serde(default)]
    pub limit: Option<u32>,
    /// If true, completed inbox to-dos are also returned. Defaults to false.
    #[serde(default)]
    pub include_completed: Option<bool>,
}

pub async fn things_list_inbox(
    state: AppState,
    args: ListInboxArgs,
) -> anyhow::Result<TodoList> {
    let params = ListInboxParams {
        include_completed: args.include_completed.unwrap_or(false),
        limit: args.limit.unwrap_or(200),
    };
    let items = list_inbox(&state.pool, params).await?;
    Ok(TodoList { items })
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct ListTodayArgs {
    /// Cap on returned rows. Defaults to 200.
    #[serde(default)]
    pub limit: Option<u32>,
}

pub async fn things_list_today(
    state: AppState,
    args: ListTodayArgs,
) -> anyhow::Result<TodoList> {
    let params = ListTodayParams {
        limit: args.limit.unwrap_or(200),
    };
    let items = list_today(&state.pool, params).await?;
    Ok(TodoList { items })
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct ListUpcomingArgs {
    /// Lower bound (exclusive) as `YYYY-MM-DD`. Defaults to today.
    #[serde(default)]
    pub from: Option<String>,
    /// Upper bound (inclusive) as `YYYY-MM-DD`. If omitted, no upper bound.
    #[serde(default)]
    pub to: Option<String>,
    /// Cap on returned rows. Defaults to 200.
    #[serde(default)]
    pub limit: Option<u32>,
}

pub async fn things_list_upcoming(
    state: AppState,
    args: ListUpcomingArgs,
) -> anyhow::Result<TodoList> {
    let params = ListUpcomingParams {
        from_iso: args.from,
        to_iso: args.to,
        limit: args.limit.unwrap_or(200),
    };
    let items = list_upcoming(&state.pool, params).await?;
    Ok(TodoList { items })
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct ListAnytimeArgs {
    /// Restrict to to-dos belonging to a specific area (directly or via project). Optional.
    #[serde(default)]
    pub area_id: Option<String>,
    /// Cap on returned rows. Defaults to 200.
    #[serde(default)]
    pub limit: Option<u32>,
}

pub async fn things_list_anytime(
    state: AppState,
    args: ListAnytimeArgs,
) -> anyhow::Result<TodoList> {
    let params = ListAnytimeParams {
        area_id: args.area_id,
        limit: args.limit.unwrap_or(200),
    };
    let items = list_anytime(&state.pool, params).await?;
    Ok(TodoList { items })
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct ListSomedayArgs {
    /// Cap on returned rows. Defaults to 200.
    #[serde(default)]
    pub limit: Option<u32>,
}

pub async fn things_list_someday(
    state: AppState,
    args: ListSomedayArgs,
) -> anyhow::Result<TodoList> {
    let params = ListSomedayParams {
        limit: args.limit.unwrap_or(200),
    };
    let items = list_someday(&state.pool, params).await?;
    Ok(TodoList { items })
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct ListLogbookArgs {
    /// Lower bound on completion date as `YYYY-MM-DD` (inclusive). Optional.
    #[serde(default)]
    pub from: Option<String>,
    /// Upper bound on completion date as `YYYY-MM-DD` (inclusive — end-of-day). Optional.
    #[serde(default)]
    pub to: Option<String>,
    /// Cap on returned rows. Defaults to 100.
    #[serde(default)]
    pub limit: Option<u32>,
}

pub async fn things_list_logbook(
    state: AppState,
    args: ListLogbookArgs,
) -> anyhow::Result<TodoList> {
    let params = ListLogbookParams {
        from_iso: args.from,
        to_iso: args.to,
        limit: args.limit.unwrap_or(100),
    };
    let items = list_logbook(&state.pool, params).await?;
    Ok(TodoList { items })
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct ListTrashArgs {
    /// Cap on returned rows. Defaults to 100.
    #[serde(default)]
    pub limit: Option<u32>,
}

pub async fn things_list_trash(
    state: AppState,
    args: ListTrashArgs,
) -> anyhow::Result<TodoList> {
    let params = ListTrashParams {
        limit: args.limit.unwrap_or(100),
    };
    let items = list_trash(&state.pool, params).await?;
    Ok(TodoList { items })
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct ListAreasArgs {}

pub async fn things_list_areas(
    state: AppState,
    _args: ListAreasArgs,
) -> anyhow::Result<AreaList> {
    let items = list_areas(&state.pool).await?;
    Ok(AreaList { items })
}

use crate::core::reader::queries::{list_projects, ListProjectsParams, ProjectStatusFilter};
use crate::core::reader::queries::{list_by_tag, ListByTagParams};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProjectStatusArg {
    #[default]
    Open,
    Done,
    All,
}

impl From<ProjectStatusArg> for ProjectStatusFilter {
    fn from(a: ProjectStatusArg) -> Self {
        match a {
            ProjectStatusArg::Open => Self::Open,
            ProjectStatusArg::Done => Self::Done,
            ProjectStatusArg::All => Self::All,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct ListProjectsArgs {
    /// Restrict to projects in a given area. Optional.
    #[serde(default)]
    pub area_id: Option<String>,
    /// `open` (default), `done`, or `all`.
    #[serde(default)]
    pub status: Option<ProjectStatusArg>,
}

pub async fn things_list_projects(
    state: AppState,
    args: ListProjectsArgs,
) -> anyhow::Result<ProjectList> {
    let params = ListProjectsParams {
        area_id: args.area_id,
        status: args.status.unwrap_or_default().into(),
    };
    let items = list_projects(&state.pool, params).await?;
    Ok(ProjectList { items })
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct ListByTagArgs {
    /// Tag identifier — either the user-facing title (`"Errand"`) or the UUID (`"tag-errand"`).
    pub tag: String,
    /// If true (default), also matches descendants of the named tag.
    #[serde(default)]
    pub recurse: Option<bool>,
    /// Cap on returned rows. Defaults to 200.
    #[serde(default)]
    pub limit: Option<u32>,
}

pub async fn things_list_by_tag(
    state: AppState,
    args: ListByTagArgs,
) -> anyhow::Result<TodoList> {
    let params = ListByTagParams {
        tag: args.tag,
        recurse: args.recurse.unwrap_or(true),
        limit: args.limit.unwrap_or(200),
    };
    let items = list_by_tag(&state.pool, params).await?;
    Ok(TodoList { items })
}
