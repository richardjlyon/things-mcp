//! Read tools that surface a single to-do.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::core::reader::queries::{get_tags_for_task, get_todo};
use crate::core::types::MaybeTodo;
use crate::state::AppState;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct GetTodoArgs {
    /// The to-do's UUID (`TMTask.uuid`).
    pub id: String,
}

pub async fn things_get_todo(
    state: AppState,
    args: GetTodoArgs,
) -> anyhow::Result<MaybeTodo> {
    let todo = get_todo(&state.pool, args.id).await?;
    Ok(MaybeTodo { todo })
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

use crate::core::writer::operation::{MoveTodoSpec, UpdateTodoSpec};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct UpdateTodoArgs {
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
    pub list_id: Option<String>,
    #[serde(default)]
    pub completed: Option<bool>,
    #[serde(default)]
    pub canceled: Option<bool>,
}

pub async fn things_update_todo(
    state: AppState,
    args: UpdateTodoArgs,
) -> anyhow::Result<WriteOutcome> {
    if args.id.trim().is_empty() {
        return Err(crate::core::error::ThingsError::InvalidInput {
            field: "id".into(),
            reason: "id must be non-empty".into(),
        }
        .into());
    }
    let op = Operation::UpdateTodo(UpdateTodoSpec {
        id: args.id.clone(),
        title: args.title.clone(),
        notes: args.notes.clone(),
        when: args.when,
        deadline: args.deadline,
        tags: args.tags,
        list_id: args.list_id,
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

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct StatusChangeArgs {
    /// UUID of the to-do to mark completed or canceled.
    pub id: String,
}

pub async fn things_complete_todo(
    state: AppState,
    args: StatusChangeArgs,
) -> anyhow::Result<WriteOutcome> {
    if args.id.trim().is_empty() {
        return Err(crate::core::error::ThingsError::InvalidInput {
            field: "id".into(),
            reason: "id must be non-empty".into(),
        }
        .into());
    }
    let op = Operation::CompleteTodo { id: args.id.clone() };
    let predicate = VerifyPredicate::StatusChange {
        id: args.id,
        want: crate::core::types::TaskStatus::Completed,
    };
    let outcome = state.writer.fire(op, Some(predicate)).await?;
    Ok(outcome)
}

pub async fn things_cancel_todo(
    state: AppState,
    args: StatusChangeArgs,
) -> anyhow::Result<WriteOutcome> {
    if args.id.trim().is_empty() {
        return Err(crate::core::error::ThingsError::InvalidInput {
            field: "id".into(),
            reason: "id must be non-empty".into(),
        }
        .into());
    }
    let op = Operation::CancelTodo { id: args.id.clone() };
    let predicate = VerifyPredicate::StatusChange {
        id: args.id,
        want: crate::core::types::TaskStatus::Canceled,
    };
    let outcome = state.writer.fire(op, Some(predicate)).await?;
    Ok(outcome)
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct MoveTodoArgs {
    /// UUID of the to-do to move.
    pub id: String,
    /// Target project or area UUID. `None` (omitted) moves to the Inbox.
    #[serde(default)]
    pub list_id: Option<String>,
}

pub async fn things_move_todo(
    state: AppState,
    args: MoveTodoArgs,
) -> anyhow::Result<WriteOutcome> {
    if args.id.trim().is_empty() {
        return Err(crate::core::error::ThingsError::InvalidInput {
            field: "id".into(),
            reason: "id must be non-empty".into(),
        }
        .into());
    }
    let op = Operation::MoveTodo(MoveTodoSpec {
        id: args.id.clone(),
        list_id: args.list_id.clone(),
    });
    let predicate = VerifyPredicate::MoveById {
        id: args.id,
        expected_list_id: args.list_id,
    };
    let outcome = state.writer.fire(op, Some(predicate)).await?;
    Ok(outcome)
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct TagAssignmentArgs {
    /// UUID of the to-do or project to attach/remove tags on. Names are
    /// not accepted — pass a uuid.
    pub id: String,
    /// Tag titles (not uuids). Non-empty.
    pub tags: Vec<String>,
}

pub async fn things_assign_tag(
    state: AppState,
    args: TagAssignmentArgs,
) -> anyhow::Result<WriteOutcome> {
    if args.id.trim().is_empty() {
        return Err(crate::core::error::ThingsError::InvalidInput {
            field: "id".into(),
            reason: "id must be non-empty".into(),
        }
        .into());
    }
    if args.tags.is_empty() {
        return Err(crate::core::error::ThingsError::InvalidInput {
            field: "tags".into(),
            reason: "tags must be non-empty".into(),
        }
        .into());
    }
    if args.tags.iter().any(|t| t.trim().is_empty()) {
        return Err(crate::core::error::ThingsError::InvalidInput {
            field: "tags".into(),
            reason: "tags must not contain empty or whitespace-only entries".into(),
        }
        .into());
    }

    // Read-modify-write: union current tags with the requested set.
    let current = get_tags_for_task(&state.pool, args.id.clone()).await?;
    let mut merged: Vec<String> = current.clone();
    for t in &args.tags {
        if !merged.iter().any(|x| x == t) {
            merged.push(t.clone());
        }
    }

    let op = Operation::UpdateTodo(UpdateTodoSpec {
        id: args.id.clone(),
        tags: Some(merged),
        ..Default::default()
    });
    // Verify the first requested tag landed; if Things merges them in one
    // write (the common case), the rest landed too.
    let predicate = VerifyPredicate::TagOnTodoById {
        id: args.id,
        tag: args.tags[0].clone(),
        present: true,
    };
    let outcome = state.writer.fire(op, Some(predicate)).await?;
    Ok(outcome)
}

pub async fn things_unassign_tag(
    state: AppState,
    args: TagAssignmentArgs,
) -> anyhow::Result<WriteOutcome> {
    if args.id.trim().is_empty() {
        return Err(crate::core::error::ThingsError::InvalidInput {
            field: "id".into(),
            reason: "id must be non-empty".into(),
        }
        .into());
    }
    if args.tags.is_empty() {
        return Err(crate::core::error::ThingsError::InvalidInput {
            field: "tags".into(),
            reason: "tags must be non-empty".into(),
        }
        .into());
    }
    if args.tags.iter().any(|t| t.trim().is_empty()) {
        return Err(crate::core::error::ThingsError::InvalidInput {
            field: "tags".into(),
            reason: "tags must not contain empty or whitespace-only entries".into(),
        }
        .into());
    }

    // Read-modify-write: filter out the requested tags.
    let current = get_tags_for_task(&state.pool, args.id.clone()).await?;
    let to_remove: std::collections::HashSet<&str> =
        args.tags.iter().map(|s| s.as_str()).collect();
    let new_set: Vec<String> = current
        .into_iter()
        .filter(|t| !to_remove.contains(t.as_str()))
        .collect();

    let op = Operation::UpdateTodo(UpdateTodoSpec {
        id: args.id.clone(),
        tags: Some(new_set),
        ..Default::default()
    });
    let predicate = VerifyPredicate::TagOnTodoById {
        id: args.id,
        tag: args.tags[0].clone(),
        present: false,
    };
    let outcome = state.writer.fire(op, Some(predicate)).await?;
    Ok(outcome)
}
