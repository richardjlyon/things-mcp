//! Read tools that surface a single to-do.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::core::reader::queries::get_todo;
use crate::core::types::TodoFull;
use crate::state::AppState;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct GetTodoArgs {
    /// The to-do's UUID (`TMTask.uuid`).
    pub id: String,
}

pub async fn things_get_todo(
    state: AppState,
    args: GetTodoArgs,
) -> anyhow::Result<Option<TodoFull>> {
    let full = get_todo(&state.pool, args.id).await?;
    Ok(full)
}
