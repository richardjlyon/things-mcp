//! `CompleteTodo { id }` + `CancelTodo { id }` — narrow status-change updates
//! that share a rendering shape (a tiny update with one boolean attribute).

use serde_json::{json, Value};

pub(crate) fn render_complete_todo(id: &str) -> Value {
    json!({
        "type": "to-do",
        "operation": "update",
        "id": id,
        "attributes": { "completed": true },
    })
}

pub(crate) fn render_cancel_todo(id: &str) -> Value {
    json!({
        "type": "to-do",
        "operation": "update",
        "id": id,
        "attributes": { "canceled": true },
    })
}

#[cfg(test)]
mod tests {
    use crate::core::writer::operation::Operation;

    #[test]
    fn complete_todo_renders_completed_true() {
        let op = Operation::CompleteTodo { id: "todo-1".into() };
        let v = op.render_json();
        assert_eq!(v["type"], "to-do");
        assert_eq!(v["operation"], "update");
        assert_eq!(v["id"], "todo-1");
        assert_eq!(v["attributes"]["completed"], true);
        // No other attributes.
        assert_eq!(v["attributes"].as_object().unwrap().len(), 1);
    }

    #[test]
    fn cancel_todo_renders_canceled_true() {
        let op = Operation::CancelTodo { id: "todo-2".into() };
        let v = op.render_json();
        assert_eq!(v["type"], "to-do");
        assert_eq!(v["operation"], "update");
        assert_eq!(v["id"], "todo-2");
        assert_eq!(v["attributes"]["canceled"], true);
        assert_eq!(v["attributes"].as_object().unwrap().len(), 1);
    }

    #[test]
    fn complete_and_cancel_action_names_and_auth() {
        let c = Operation::CompleteTodo { id: "x".into() };
        let x = Operation::CancelTodo { id: "x".into() };
        assert_eq!(c.action_name(), "complete_todo");
        assert_eq!(x.action_name(), "cancel_todo");
        assert!(c.requires_auth_token());
        assert!(x.requires_auth_token());
    }

    #[test]
    fn complete_distinct_from_cancel() {
        let c = Operation::CompleteTodo { id: "x".into() }.render_json();
        let x = Operation::CancelTodo { id: "x".into() }.render_json();
        // The two MUST emit different boolean keys — they are not interchangeable.
        assert!(c["attributes"].as_object().unwrap().contains_key("completed"));
        assert!(x["attributes"].as_object().unwrap().contains_key("canceled"));
        assert!(!c["attributes"].as_object().unwrap().contains_key("canceled"));
        assert!(!x["attributes"].as_object().unwrap().contains_key("completed"));
    }
}
