//! Read tools that surface a single project.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::core::reader::queries::get_project;
use crate::core::types::ProjectFull;
use crate::state::AppState;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct GetProjectArgs {
    /// The project's UUID (`TMTask.uuid` where `type = 1`).
    pub id: String,
}

pub async fn things_get_project(
    state: AppState,
    args: GetProjectArgs,
) -> anyhow::Result<Option<ProjectFull>> {
    let full = get_project(&state.pool, args.id).await?;
    Ok(full)
}
