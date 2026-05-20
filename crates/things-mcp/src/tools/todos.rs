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

use std::time::{SystemTime, UNIX_EPOCH};

use crate::core::writer::operation::{AddTodoSpec, Operation};
use crate::core::writer::outcome::WriteOutcome;
use crate::core::writer::verify::VerifyPredicate;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct AddTodoArgs {
    /// To-do title. Required, non-empty.
    pub title: String,
    /// Free-text notes (optional).
    #[serde(default)]
    pub notes: Option<String>,
    /// `"today"`, `"tomorrow"`, `"evening"`, `"anytime"`, `"someday"`, or an
    /// ISO date / timestamp. Optional.
    #[serde(default)]
    pub when: Option<String>,
    /// ISO `YYYY-MM-DD` deadline. Optional.
    #[serde(default)]
    pub deadline: Option<String>,
    /// Tag titles to attach to the new to-do. Optional.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Checklist item titles, in display order. Optional.
    #[serde(default)]
    pub checklist_items: Vec<String>,
    /// Project or area UUID this to-do should belong to. Optional.
    #[serde(default)]
    pub list_id: Option<String>,
    /// Heading UUID, if filing under a specific heading inside a project. Optional.
    #[serde(default)]
    pub heading_id: Option<String>,
}

pub async fn things_add_todo(
    state: AppState,
    args: AddTodoArgs,
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
    let op = Operation::AddTodo(AddTodoSpec {
        title: args.title.clone(),
        notes: args.notes,
        when: args.when,
        deadline: args.deadline,
        tags: args.tags,
        checklist_items: args.checklist_items,
        list_id: args.list_id,
        heading_id: args.heading_id,
    });
    let predicate = VerifyPredicate::CreateByTitle {
        title: args.title,
        since_unix,
        kind: crate::core::types::TaskKind::Todo,
    };
    let outcome = state.writer.fire(op, Some(predicate)).await?;
    Ok(outcome)
}
