//! `Operation` — typed write operations, each capable of rendering itself
//! as a single Things JSON URL operation element.

pub mod add_project;
pub mod add_todo;
pub mod status_change;
pub mod update_project;
pub mod update_todo;

pub use add_project::AddProjectSpec;
pub use add_todo::AddTodoSpec;
pub use update_project::UpdateProjectSpec;
pub use update_todo::UpdateTodoSpec;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Operation {
    AddTodo(AddTodoSpec),
    AddProject(AddProjectSpec),
    UpdateTodo(UpdateTodoSpec),
    UpdateProject(UpdateProjectSpec),
    CompleteTodo { id: String },
    CancelTodo { id: String },
}

impl Operation {
    pub fn action_name(&self) -> &'static str {
        match self {
            Operation::AddTodo(_) => "add_todo",
            Operation::AddProject(_) => "add_project",
            Operation::UpdateTodo(_) => "update_todo",
            Operation::UpdateProject(_) => "update_project",
            Operation::CompleteTodo { .. } => "complete_todo",
            Operation::CancelTodo { .. } => "cancel_todo",
        }
    }

    pub fn requires_auth_token(&self) -> bool {
        match self {
            Operation::AddTodo(_) => false,
            Operation::AddProject(_) => false,
            Operation::UpdateTodo(_) => true,
            Operation::UpdateProject(_) => true,
            Operation::CompleteTodo { .. } => true,
            Operation::CancelTodo { .. } => true,
        }
    }

    pub fn render_json(&self) -> Value {
        match self {
            Operation::AddTodo(spec) => add_todo::render_add_todo(spec),
            Operation::AddProject(spec) => add_project::render_add_project(spec),
            Operation::UpdateTodo(spec) => update_todo::render_update_todo(spec),
            Operation::UpdateProject(spec) => update_project::render_update_project(spec),
            Operation::CompleteTodo { id } => status_change::render_complete_todo(id),
            Operation::CancelTodo { id } => status_change::render_cancel_todo(id),
        }
    }
}
