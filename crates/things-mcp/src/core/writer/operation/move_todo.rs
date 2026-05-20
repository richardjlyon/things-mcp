//! `MoveTodoSpec` — relocate a to-do under a project, area, or to the Inbox.
//! Renders as a Things update with a `list-id` attribute. `None` maps to the
//! special `"inbox"` value (the Things URL scheme's sentinel for no parent).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MoveTodoSpec {
    pub id: String,
    /// `Some(uuid)` = move to that project or area. `None` = move to Inbox.
    pub list_id: Option<String>,
}

pub(crate) fn render_move_todo(spec: &MoveTodoSpec) -> Value {
    let target = spec.list_id.clone().unwrap_or_else(|| "inbox".to_string());
    json!({
        "type": "to-do",
        "operation": "update",
        "id": spec.id,
        "attributes": { "list-id": target },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::writer::operation::Operation;

    #[test]
    fn move_todo_to_named_list() {
        let op = Operation::MoveTodo(MoveTodoSpec {
            id: "todo-1".into(),
            list_id: Some("proj-1".into()),
        });
        let v = op.render_json();
        assert_eq!(v["type"], "to-do");
        assert_eq!(v["operation"], "update");
        assert_eq!(v["id"], "todo-1");
        assert_eq!(v["attributes"]["list-id"], "proj-1");
    }

    #[test]
    fn move_todo_to_inbox_uses_inbox_sentinel() {
        let op = Operation::MoveTodo(MoveTodoSpec {
            id: "todo-1".into(),
            list_id: None,
        });
        let v = op.render_json();
        assert_eq!(v["attributes"]["list-id"], "inbox");
    }
}
