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

use std::time::{SystemTime, UNIX_EPOCH};

use crate::core::writer::operation::{AddProjectSpec, Operation, UpdateProjectSpec};
use crate::core::writer::outcome::WriteOutcome;
use crate::core::writer::verify::VerifyPredicate;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct AddProjectArgs {
    /// Project title. Required, non-empty.
    pub title: String,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub when: Option<String>,
    #[serde(default)]
    pub deadline: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Parent area UUID.
    #[serde(default)]
    pub area_id: Option<String>,
    /// Initial heading titles. Order preserved.
    #[serde(default)]
    pub headings: Vec<String>,
}

pub async fn things_add_project(
    state: AppState,
    args: AddProjectArgs,
) -> anyhow::Result<WriteOutcome> {
    if args.title.trim().is_empty() {
        return Err(crate::core::error::ThingsError::InvalidInput {
            field: "title".into(),
            reason: "title must be non-empty".into(),
        }
        .into());
    }
    let since_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    let op = Operation::AddProject(AddProjectSpec {
        title: args.title.clone(),
        notes: args.notes,
        when: args.when,
        deadline: args.deadline,
        tags: args.tags,
        area_id: args.area_id,
        todos: Vec::new(),
        headings: args.headings,
    });
    let predicate = VerifyPredicate::CreateByTitle {
        title: args.title,
        since_unix,
        kind: crate::core::types::TaskKind::Project,
    };
    let outcome = state.writer.fire(op, Some(predicate)).await?;
    Ok(outcome)
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct UpdateProjectArgs {
    /// UUID of the project to update.
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub when: Option<String>,
    #[serde(default)]
    pub deadline: Option<String>,
    /// `None` = leave tags unchanged. `Some(vec![])` = clear all tags.
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub area_id: Option<String>,
    #[serde(default)]
    pub completed: Option<bool>,
    #[serde(default)]
    pub canceled: Option<bool>,
}

pub async fn things_update_project(
    state: AppState,
    args: UpdateProjectArgs,
) -> anyhow::Result<WriteOutcome> {
    if args.id.trim().is_empty() {
        return Err(crate::core::error::ThingsError::InvalidInput {
            field: "id".into(),
            reason: "id must be non-empty".into(),
        }
        .into());
    }
    let op = Operation::UpdateProject(UpdateProjectSpec {
        id: args.id.clone(),
        title: args.title.clone(),
        notes: args.notes.clone(),
        when: args.when,
        deadline: args.deadline,
        tags: args.tags,
        area_id: args.area_id,
        completed: args.completed,
        canceled: args.canceled,
    });
    let predicate = VerifyPredicate::UpdateById {
        id: args.id,
        expected_title: args.title,
        expected_notes: args.notes,
    };
    let outcome = state.writer.fire(op, Some(predicate)).await?;
    Ok(outcome)
}
