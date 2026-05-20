//! Read tools that surface a Things list view.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::core::reader::queries::{list_inbox, ListInboxParams};
use crate::core::types::TodoSummary;
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
) -> anyhow::Result<Vec<TodoSummary>> {
    let params = ListInboxParams {
        include_completed: args.include_completed.unwrap_or(false),
        limit: args.limit.unwrap_or(200),
    };
    let rows = list_inbox(&state.pool, params).await?;
    Ok(rows)
}
