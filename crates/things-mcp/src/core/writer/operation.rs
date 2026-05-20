//! `Operation` — typed write operations, each capable of rendering itself
//! as a single Things JSON URL operation element.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Operation {
    AddTodo(AddTodoSpec),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AddTodoSpec {
    pub title: String,
    pub notes: Option<String>,
    /// `"today"`, `"tomorrow"`, `"evening"`, `"anytime"`, `"someday"`,
    /// or an ISO date / timestamp.
    pub when: Option<String>,
    /// ISO `YYYY-MM-DD`.
    pub deadline: Option<String>,
    pub tags: Vec<String>,
    pub checklist_items: Vec<String>,
    /// Project or area UUID this to-do belongs to.
    pub list_id: Option<String>,
    /// Heading UUID, if the to-do should be filed under a specific heading
    /// inside a project.
    pub heading_id: Option<String>,
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
            Operation::AddTodo(spec) => render_add_todo(spec),
        }
    }
}

fn render_add_todo(spec: &AddTodoSpec) -> Value {
    let mut attributes = serde_json::Map::new();
    attributes.insert("title".into(), Value::String(spec.title.clone()));
    if let Some(notes) = spec.notes.as_ref() {
        attributes.insert("notes".into(), Value::String(notes.clone()));
    }
    if let Some(when) = spec.when.as_ref() {
        attributes.insert("when".into(), Value::String(when.clone()));
    }
    if let Some(deadline) = spec.deadline.as_ref() {
        attributes.insert("deadline".into(), Value::String(deadline.clone()));
    }
    if !spec.tags.is_empty() {
        attributes.insert(
            "tags".into(),
            Value::Array(spec.tags.iter().map(|t| Value::String(t.clone())).collect()),
        );
    }
    if !spec.checklist_items.is_empty() {
        attributes.insert(
            "checklist-items".into(),
            Value::Array(
                spec.checklist_items
                    .iter()
                    .map(|t| {
                        json!({
                            "type": "checklist-item",
                            "attributes": { "title": t }
                        })
                    })
                    .collect(),
            ),
        );
    }
    if let Some(id) = spec.list_id.as_ref() {
        attributes.insert("list-id".into(), Value::String(id.clone()));
    }
    if let Some(id) = spec.heading_id.as_ref() {
        attributes.insert("heading".into(), Value::String(id.clone()));
    }

    json!({
        "type": "to-do",
        "attributes": Value::Object(attributes),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_todo_minimal_renders_title_only() {
        let op = Operation::AddTodo(AddTodoSpec {
            title: "Buy milk".into(),
            ..Default::default()
        });
        let v = op.render_json();
        assert_eq!(v["type"], "to-do");
        assert_eq!(v["attributes"]["title"], "Buy milk");
        // No spurious keys for empty options.
        let attrs = v["attributes"].as_object().unwrap();
        assert_eq!(attrs.len(), 1);
        assert!(!attrs.contains_key("notes"));
        assert!(!attrs.contains_key("tags"));
        assert!(!attrs.contains_key("checklist-items"));
    }

    #[test]
    fn add_todo_full_renders_every_field() {
        let op = Operation::AddTodo(AddTodoSpec {
            title: "Plan release".into(),
            notes: Some("Coordinate with QA".into()),
            when: Some("today".into()),
            deadline: Some("2026-06-01".into()),
            tags: vec!["Work".into(), "Urgent".into()],
            checklist_items: vec!["Draft notes".into(), "Cut RC".into()],
            list_id: Some("proj-42".into()),
            heading_id: Some("head-7".into()),
        });
        let v = op.render_json();
        let attrs = v["attributes"].as_object().unwrap();
        assert_eq!(attrs["title"], "Plan release");
        assert_eq!(attrs["notes"], "Coordinate with QA");
        assert_eq!(attrs["when"], "today");
        assert_eq!(attrs["deadline"], "2026-06-01");
        assert_eq!(attrs["tags"], serde_json::json!(["Work", "Urgent"]));
        assert_eq!(attrs["list-id"], "proj-42");
        assert_eq!(attrs["heading"], "head-7");
        let checklist = attrs["checklist-items"].as_array().unwrap();
        assert_eq!(checklist.len(), 2);
        assert_eq!(checklist[0]["type"], "checklist-item");
        assert_eq!(checklist[0]["attributes"]["title"], "Draft notes");
    }

    #[test]
    fn action_name_and_auth_requirement() {
        let op = Operation::AddTodo(AddTodoSpec {
            title: "x".into(),
            ..Default::default()
        });
        assert_eq!(op.action_name(), "add_todo");
        assert!(!op.requires_auth_token(), "creates do not require auth-token");
    }
}
