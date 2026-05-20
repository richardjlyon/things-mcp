//! `AddProjectSpec` and its JSON render. Creates a Things project, optionally
//! with initial headings and to-dos nested inside via Things' `items` array.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::core::writer::operation::add_todo::AddTodoSpec;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AddProjectSpec {
    pub title: String,
    pub notes: Option<String>,
    pub when: Option<String>,
    pub deadline: Option<String>,
    pub tags: Vec<String>,
    /// Parent area UUID. Optional — projects live in "no area" if omitted.
    pub area_id: Option<String>,
    /// Initial to-dos to nest inside the project. Order preserved.
    pub todos: Vec<AddTodoSpec>,
    /// Initial heading titles. Order preserved. Renders before todos in the
    /// items[] array — a Things UX convention, not a hard rule.
    pub headings: Vec<String>,
}

pub(crate) fn render_add_project(spec: &AddProjectSpec) -> Value {
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
    if let Some(id) = spec.area_id.as_ref() {
        attributes.insert("area-id".into(), Value::String(id.clone()));
    }

    // items[] = headings first, then to-dos. Order matches the Things app's
    // typical project layout.
    if !spec.headings.is_empty() || !spec.todos.is_empty() {
        let mut items: Vec<Value> = Vec::with_capacity(spec.headings.len() + spec.todos.len());
        for h in &spec.headings {
            items.push(json!({
                "type": "heading",
                "attributes": { "title": h }
            }));
        }
        for t in &spec.todos {
            // Reuse AddTodoSpec's render via Operation dispatch — but we only
            // need the element shape, not the wrapped Operation. Inline the
            // render here to avoid coupling enum dispatch into the project
            // render.
            items.push(crate::core::writer::operation::add_todo::render_add_todo(t));
        }
        attributes.insert("items".into(), Value::Array(items));
    }

    json!({
        "type": "project",
        "attributes": Value::Object(attributes),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::writer::operation::Operation;

    #[test]
    fn add_project_minimal_renders_title_only() {
        let op = Operation::AddProject(AddProjectSpec {
            title: "Launch website".into(),
            ..Default::default()
        });
        let v = op.render_json();
        assert_eq!(v["type"], "project");
        assert_eq!(v["attributes"]["title"], "Launch website");
        let attrs = v["attributes"].as_object().unwrap();
        assert_eq!(attrs.len(), 1, "only `title` should be set for minimal project");
        assert!(!attrs.contains_key("items"));
        assert!(!attrs.contains_key("area-id"));
    }

    #[test]
    fn add_project_full_with_nested_items() {
        let op = Operation::AddProject(AddProjectSpec {
            title: "Q3 launch".into(),
            notes: Some("Coordinate with marketing".into()),
            when: Some("anytime".into()),
            deadline: Some("2026-09-30".into()),
            tags: vec!["Work".into()],
            area_id: Some("area-2".into()),
            todos: vec![
                AddTodoSpec {
                    title: "Draft press release".into(),
                    ..Default::default()
                },
            ],
            headings: vec!["Design".into(), "QA".into()],
        });
        let v = op.render_json();
        let attrs = v["attributes"].as_object().unwrap();
        assert_eq!(attrs["title"], "Q3 launch");
        assert_eq!(attrs["notes"], "Coordinate with marketing");
        assert_eq!(attrs["area-id"], "area-2");
        assert_eq!(attrs["tags"], serde_json::json!(["Work"]));
        let items = attrs["items"].as_array().unwrap();
        // 2 headings + 1 to-do, headings first.
        assert_eq!(items.len(), 3);
        assert_eq!(items[0]["type"], "heading");
        assert_eq!(items[0]["attributes"]["title"], "Design");
        assert_eq!(items[1]["type"], "heading");
        assert_eq!(items[1]["attributes"]["title"], "QA");
        assert_eq!(items[2]["type"], "to-do");
        assert_eq!(items[2]["attributes"]["title"], "Draft press release");
    }
}
