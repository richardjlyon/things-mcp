//! `UpdateProjectSpec` and its JSON render. Renders a Things project
//! "update" operation.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateProjectSpec {
    pub id: String,
    pub title: Option<String>,
    pub notes: Option<String>,
    pub when: Option<String>,
    pub deadline: Option<String>,
    pub tags: Option<Vec<String>>,
    /// Parent area UUID. `Some("inbox")` not meaningful for projects — pass an
    /// area UUID or omit.
    pub area_id: Option<String>,
    pub completed: Option<bool>,
    pub canceled: Option<bool>,
}

pub(crate) fn render_update_project(spec: &UpdateProjectSpec) -> Value {
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
    if let Some(v) = spec.area_id.as_ref() {
        attributes.insert("area-id".into(), Value::String(v.clone()));
    }
    if let Some(v) = spec.completed {
        attributes.insert("completed".into(), Value::Bool(v));
    }
    if let Some(v) = spec.canceled {
        attributes.insert("canceled".into(), Value::Bool(v));
    }

    json!({
        "type": "project",
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
    fn update_project_minimal_only_id() {
        let op = Operation::UpdateProject(UpdateProjectSpec {
            id: "proj-1".into(),
            ..Default::default()
        });
        let v = op.render_json();
        assert_eq!(v["type"], "project");
        assert_eq!(v["operation"], "update");
        assert_eq!(v["id"], "proj-1");
        assert_eq!(v["attributes"].as_object().unwrap().len(), 0);
    }

    #[test]
    fn update_project_full_renders_all_populated_fields() {
        let op = Operation::UpdateProject(UpdateProjectSpec {
            id: "proj-1".into(),
            title: Some("Renamed".into()),
            notes: Some("Updated notes".into()),
            when: Some("today".into()),
            deadline: Some("2026-12-31".into()),
            tags: Some(vec!["Work".into()]),
            area_id: Some("area-2".into()),
            completed: Some(true),
            canceled: None,
        });
        let v = op.render_json();
        assert_eq!(v["id"], "proj-1");
        let attrs = v["attributes"].as_object().unwrap();
        assert_eq!(attrs["title"], "Renamed");
        assert_eq!(attrs["notes"], "Updated notes");
        assert_eq!(attrs["when"], "today");
        assert_eq!(attrs["deadline"], "2026-12-31");
        assert_eq!(attrs["tags"], serde_json::json!(["Work"]));
        assert_eq!(attrs["area-id"], "area-2");
        assert_eq!(attrs["completed"], true);
        assert!(!attrs.contains_key("canceled"), "None should not render");
    }
}
