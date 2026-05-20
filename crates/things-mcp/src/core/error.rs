//! Domain errors. All variants serialise to a stable structured form so MCP
//! callers see typed errors, never bare strings.

use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ThingsError {
    #[error(
        "missing Things auth-token (set THINGS_AUTH_TOKEN or config.toml [things].auth_token)"
    )]
    MissingAuthToken { hint: String },

    #[error("Things SQLite schema is incompatible — missing columns: {missing:?}")]
    SchemaIncompatible {
        missing: Vec<String>,
        things_version_guess: Option<String>,
    },

    #[error("Things database is locked; retry in {retry_in_ms} ms")]
    DbLocked { retry_in_ms: u32 },

    #[error("write was unverified after {elapsed_ms} ms; payload echo follows")]
    WriteUnverified {
        payload_echo: String,
        elapsed_ms: u32,
    },

    #[error("unsupported recurrence pattern '{pattern}'; supported: {supported:?}")]
    UnsupportedRecurrence {
        pattern: String,
        supported: Vec<String>,
    },

    #[error("operation not allowed on repeating item {id} (field '{field}')")]
    OperationNotAllowedOnRepeatingItem { id: String, field: String },

    #[error("dry-run only (test-DB mode): would have opened {url}")]
    DryRun {
        url: String,
        payload: serde_json::Value,
    },

    #[error("Things rejected the auth-token (writes will not succeed)")]
    AuthTokenRejected,

    #[error("AppleScript exited {exit}: {stderr}")]
    AppleScriptFailed { stderr: String, exit: i32 },

    #[error("Things app is not running")]
    ThingsAppNotRunning,

    #[error("invalid input for '{field}': {reason}")]
    InvalidInput { field: String, reason: String },

    #[error("io: {0}")]
    Io(String),

    #[error("sqlite: {0}")]
    Sqlite(String),
}

impl From<std::io::Error> for ThingsError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

impl From<rusqlite::Error> for ThingsError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sqlite(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_auth_token_serialises_to_tagged_json() {
        let err = ThingsError::MissingAuthToken {
            hint: "set THINGS_AUTH_TOKEN".into(),
        };
        let v = serde_json::to_value(&err).unwrap();
        assert_eq!(v["kind"], "missing_auth_token");
        assert_eq!(v["hint"], "set THINGS_AUTH_TOKEN");
    }

    #[test]
    fn schema_incompatible_carries_missing_columns() {
        let err = ThingsError::SchemaIncompatible {
            missing: vec!["TMTask.uuid".into()],
            things_version_guess: None,
        };
        let v = serde_json::to_value(&err).unwrap();
        assert_eq!(v["kind"], "schema_incompatible");
        assert_eq!(v["missing"][0], "TMTask.uuid");
    }
}
