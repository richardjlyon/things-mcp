//! `Operation` — typed write operations, each capable of rendering itself
//! as a single Things JSON URL operation element.

pub mod add_todo;

pub use add_todo::AddTodoSpec;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Operation {
    AddTodo(AddTodoSpec),
}

impl Operation {
    /// Stable snake-case action name surfaced in `WriteOutcome.action`.
    pub fn action_name(&self) -> &'static str {
        match self {
            Operation::AddTodo(_) => "add_todo",
        }
    }

    /// `true` iff this operation type needs Things' auth-token (i.e. it's an
    /// `update`). Creates pass through without one.
    pub fn requires_auth_token(&self) -> bool {
        match self {
            Operation::AddTodo(_) => false,
        }
    }

    /// Render this operation as a single element of the JSON array payload
    /// Things expects in `things:///json?data=…`.
    pub fn render_json(&self) -> Value {
        match self {
            Operation::AddTodo(spec) => add_todo::render_add_todo(spec),
        }
    }
}
