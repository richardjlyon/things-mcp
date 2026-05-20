//! `BulkRawSpec` — pass an arbitrary JSON array of Things URL scheme
//! operation objects straight through `build_url`. Power tool, intended
//! for callers that already have well-formed Things payloads.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BulkRawSpec {
    /// Each element is rendered straight into the data[] array of the URL
    /// payload. The chassis does NOT validate per-element structure.
    pub operations: Vec<Value>,
}

/// The bulk variant doesn't fit the "one operation = one JSON element" model
/// the rest of the enum uses. The render_json method returns the FIRST element
/// to keep the trait shape uniform; for the full payload, callers must
/// extract `BulkRawSpec.operations` directly. In practice, this is invisible
/// because `build_url` is also extended to special-case the BulkRaw variant.
///
/// We choose this shape because the Operation enum is intentionally narrow
/// (one variant = one rendered JSON object); making render_json return Value
/// for non-bulk variants and Vec<Value> for bulk would force every caller
/// into a match. Keeping the trait uniform and special-casing in build_url
/// is the lesser evil — and the build_url change is one if-let.
pub(crate) fn render_bulk_first(spec: &BulkRawSpec) -> Value {
    spec.operations
        .first()
        .cloned()
        .unwrap_or_else(|| Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::writer::operation::Operation;

    #[test]
    fn bulk_action_name_and_auth_requirement() {
        let op = Operation::BulkRaw(BulkRawSpec {
            operations: vec![serde_json::json!({"type": "to-do", "attributes": {"title": "x"}})],
        });
        assert_eq!(op.action_name(), "bulk_json");
        // Bulk is conservatively gated as IF it needs auth — the chassis
        // can't introspect the payload, so it errs on the safe side: requires
        // the token to be present if it was configured. The Writer::fire
        // logic gates this differently than other ops; see the auth-gate
        // discussion in core/writer/writer.rs.
        assert!(op.requires_auth_token());
    }

    #[test]
    fn bulk_render_returns_first_element() {
        // render_json on bulk returns the first element. The complete batch
        // is composed by build_url, not by Operation::render_json.
        let op = Operation::BulkRaw(BulkRawSpec {
            operations: vec![
                serde_json::json!({"type": "to-do", "attributes": {"title": "A"}}),
                serde_json::json!({"type": "to-do", "attributes": {"title": "B"}}),
            ],
        });
        let v = op.render_json();
        assert_eq!(v["attributes"]["title"], "A");
    }
}
