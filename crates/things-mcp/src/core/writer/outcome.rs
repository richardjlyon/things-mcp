//! `WriteOutcome` — what every write tool returns. Carries the
//! verification result and timing so the LLM can reason about whether
//! its action took effect.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WriteOutcome {
    /// UUID of the affected row. For create operations, populated from the
    /// verified row. For updates/status-changes, echoes the input id. `None`
    /// when verify timed out without a match.
    pub id: Option<String>,
    /// Snake-case action name — `"add_todo"`, `"update_todo"`, etc.
    /// Sourced from `Operation::action_name()`.
    pub action: String,
    /// `true` iff `verify()` returned `VerifyOutcome::Verified`.
    pub verified: bool,
    /// `true` when the writer short-circuited at the dry-run safety gate.
    pub dry_run: bool,
    /// Total latency (open call + verify), milliseconds. `0` when dry-run.
    pub latency_ms: u64,
}
