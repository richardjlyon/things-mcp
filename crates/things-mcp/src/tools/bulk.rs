//! Bulk write tool. Pipes a raw JSON array of Things URL scheme operation
//! objects through `build_url` and the executor without per-element
//! verification. Power tool — described as destructive in MCP annotations
//! because the chassis cannot reason about what the LLM is asking Things
//! to do.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::core::writer::operation::{BulkRawSpec, Operation};
use crate::core::writer::outcome::WriteOutcome;
use crate::state::AppState;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct BulkJsonArgs {
    /// Things URL scheme operation objects. Each element must be a JSON
    /// object — primitives or arrays are rejected. Max 250 elements per
    /// the Things rate-limit guidance.
    pub operations: Vec<serde_json::Value>,
}

pub async fn things_bulk_json(
    state: AppState,
    args: BulkJsonArgs,
) -> anyhow::Result<WriteOutcome> {
    // Pre-condition: non-empty, within rate limit.
    if args.operations.is_empty() {
        return Err(crate::core::error::ThingsError::InvalidInput {
            field: "operations".into(),
            reason: "operations must be non-empty".into(),
        }
        .into());
    }
    if args.operations.len() > 250 {
        return Err(crate::core::error::ThingsError::InvalidInput {
            field: "operations".into(),
            reason: format!(
                "operations exceeds Things rate limit (max 250, got {})",
                args.operations.len()
            ),
        }
        .into());
    }
    // Pre-condition: each element must be a JSON object.
    for (i, op) in args.operations.iter().enumerate() {
        if !op.is_object() {
            return Err(crate::core::error::ThingsError::InvalidInput {
                field: format!("operations[{i}]"),
                reason: "each element must be a JSON object".into(),
            }
            .into());
        }
    }
    let op = Operation::BulkRaw(BulkRawSpec {
        operations: args.operations,
    });
    // No verify predicate — bulk is fire-and-forget. The WriteOutcome.verified
    // field is always false; callers needing strict verification should use
    // the individual tools.
    let outcome = state.writer.fire(op, None).await?;
    Ok(outcome)
}
