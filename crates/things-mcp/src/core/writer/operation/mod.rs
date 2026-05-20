//! `Operation` — typed write operations, each capable of rendering itself
//! as a single Things JSON URL operation element.

pub mod add_project;
pub mod add_todo;

pub use add_project::AddProjectSpec;
pub use add_todo::AddTodoSpec;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Operation {
    AddTodo(AddTodoSpec),
    AddProject(AddProjectSpec),
}

impl Operation {
    pub fn action_name(&self) -> &'static str {
        match self {
            Operation::AddTodo(_) => "add_todo",
            Operation::AddProject(_) => "add_project",
        }
    }

    pub fn requires_auth_token(&self) -> bool {
        match self {
            Operation::AddTodo(_) => false,
            Operation::AddProject(_) => false,
        }
    }

    pub fn render_json(&self) -> Value {
        match self {
            Operation::AddTodo(spec) => add_todo::render_add_todo(spec),
            Operation::AddProject(spec) => add_project::render_add_project(spec),
        }
    }
}
