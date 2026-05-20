//! `UpdateTodoSpec` and its JSON render. Renders a Things "update" operation
//! (`"operation": "update"`) with the top-level `id` and only populated
//! attributes in `attributes{}`.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateTodoSpec {
    /// UUID of the to-do to update.
    pub id: String,
    /// `None` = leave field unchanged. `Some(value)` = set field to value.
    pub title: Option<String>,
    pub notes: Option<String>,
    pub when: Option<String>,
    pub deadline: Option<String>,
    /// `None` = leave tags unchanged. `Some(vec![])` = clear all tags.
    /// `Some(non_empty)` = replace tags with the given set.
    pub tags: Option<Vec<String>>,
    /// Project or area UUID. `None` = leave alone. `Some("inbox")` = move
    /// to Inbox. `Some(uuid)` = move under the given list.
    pub list_id: Option<String>,
    /// Set `true` to mark the to-do completed; `false` to un-complete.
    pub completed: Option<bool>,
    /// Set `true` to mark canceled; `false` to un-cancel.
    pub canceled: Option<bool>,
}

pub(crate) fn render_update_todo(spec: &UpdateTodoSpec) -> Value {
    let mut attributes = serde_json::Map::new();
    if let Some(v) = spec.title.as_ref() {
        attributes.insert("title".into(), Value::String(v.clone()));
    }
    if let Some(v) = spec.notes.as_ref() {
        attributes.insert("notes".into(), Value::String(v.clone()));
    }
    if let Some(v) = spec.when.as_ref() {
        attributes.insert("when".into(), Value::String(v.clone()));
    }
    if let Some(v) = spec.deadline.as_ref() {
        attributes.insert("deadline".into(), Value::String(v.clone()));
    }
    if let Some(tags) = spec.tags.as_ref() {
        attributes.insert(
            "tags".into(),
            Value::Array(tags.iter().map(|t| Value::String(t.clone())).collect()),
        );
    }
    if let Some(v) = spec.list_id.as_ref() {
        attributes.insert("list-id".into(), Value::String(v.clone()));
    }
    if let Some(v) = spec.completed {
        attributes.insert("completed".into(), Value::Bool(v));
    }
    if let Some(v) = spec.canceled {
        attributes.insert("canceled".into(), Value::Bool(v));
    }

    json!({
        "type": "to-do",
        "operation": "update",
        "id": spec.id,
        "attributes": Value::Object(attributes),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::writer::operation::Operation;

    #[test]
    fn update_todo_minimal_only_id_no_attributes() {
        let op = Operation::UpdateTodo(UpdateTodoSpec {
            id: "todo-1".into(),
            ..Default::default()
        });
        let v = op.render_json();
        assert_eq!(v["type"], "to-do");
        assert_eq!(v["operation"], "update");
        assert_eq!(v["id"], "todo-1");
        // Empty attributes object — Things treats this as a no-op update.
        assert_eq!(v["attributes"].as_object().unwrap().len(), 0);
    }

    #[test]
    fn update_todo_full_renders_all_populated_fields() {
        let op = Operation::UpdateTodo(UpdateTodoSpec {
            id: "todo-1".into(),
            title: Some("New title".into()),
            notes: Some("New notes".into()),
            when: Some("today".into()),
            deadline: Some("2026-12-31".into()),
            tags: Some(vec!["Tag A".into(), "Tag B".into()]),
            list_id: Some("proj-1".into()),
            completed: Some(true),
            canceled: Some(false),
        });
        let v = op.render_json();
        assert_eq!(v["id"], "todo-1");
        let attrs = v["attributes"].as_object().unwrap();
        assert_eq!(attrs["title"], "New title");
        assert_eq!(attrs["notes"], "New notes");
        assert_eq!(attrs["when"], "today");
        assert_eq!(attrs["deadline"], "2026-12-31");
        assert_eq!(attrs["tags"], serde_json::json!(["Tag A", "Tag B"]));
        assert_eq!(attrs["list-id"], "proj-1");
        assert_eq!(attrs["completed"], true);
        assert_eq!(attrs["canceled"], false);
    }
}
