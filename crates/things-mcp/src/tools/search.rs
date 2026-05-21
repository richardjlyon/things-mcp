//! MCP tool: free-text + structured search over to-dos.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::core::reader::queries::{search, SearchParams};
use crate::core::types::TodoList;
use crate::state::AppState;
use crate::tools::lists::ProjectStatusArg;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct SearchArgs {
    /// Free-text query, matched against the to-do's title and notes
    /// (case-insensitive substring; no boolean / wildcard syntax).
    #[serde(default)]
    pub query: Option<String>,
    /// Tag titles or UUIDs to match. OR-semantic — an item with any listed tag is returned.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Restrict to to-dos in a specific area (directly or via project). Optional.
    #[serde(default)]
    pub area_id: Option<String>,
    /// Restrict to to-dos in a specific project. Optional.
    #[serde(default)]
    pub project_id: Option<String>,
    /// `open` (default), `done`, or `all`.
    #[serde(default)]
    pub status: Option<ProjectStatusArg>,
    /// ISO `YYYY-MM-DD`. Inclusive upper bound on `deadline`. Optional.
    #[serde(default)]
    pub due_before: Option<String>,
    /// ISO `YYYY-MM-DD`. Inclusive lower bound on `deadline`. Optional.
    #[serde(default)]
    pub due_after: Option<String>,
    /// ISO `YYYY-MM-DD`. Inclusive upper bound on `startDate`. Optional.
    #[serde(default)]
    pub scheduled_before: Option<String>,
    /// ISO `YYYY-MM-DD`. Inclusive lower bound on `startDate`. Optional.
    #[serde(default)]
    pub scheduled_after: Option<String>,
    /// Cap on returned rows. Defaults to 50.
    #[serde(default)]
    pub limit: Option<u32>,
}

pub async fn things_search(
    state: AppState,
    args: SearchArgs,
) -> anyhow::Result<TodoList> {
    let params = SearchParams {
        query: args.query,
        tags: args.tags,
        area_id: args.area_id,
        project_id: args.project_id,
        status: args.status.unwrap_or_default().into(),
        due_before: args.due_before,
        due_after: args.due_after,
        scheduled_before: args.scheduled_before,
        scheduled_after: args.scheduled_after,
        limit: args.limit.unwrap_or(50),
    };
    let items = search(&state.pool, params).await?;
    Ok(TodoList { items })
}
