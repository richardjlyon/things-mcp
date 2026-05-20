//! Domain types returned from read tools and write outcomes.
//!
//! Field shapes mirror Things' SQLite columns where it matters (status, type,
//! start) and use friendly Rust enums everywhere else. Dates are surfaced as
//! ISO-8601 strings to keep MCP outputs portable.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Open,
    Canceled,
    Completed,
}

impl TaskStatus {
    pub fn from_sqlite(n: i64) -> Self {
        // Things' TMTask.status: 0=incomplete, 2=canceled, 3=completed
        match n {
            3 => Self::Completed,
            2 => Self::Canceled,
            _ => Self::Open,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    Todo,
    Project,
    Heading,
}

impl TaskKind {
    pub fn from_sqlite(n: i64) -> Self {
        // Things' TMTask.type: 0=todo, 1=project, 2=heading
        match n {
            1 => Self::Project,
            2 => Self::Heading,
            _ => Self::Todo,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StartBucket {
    Inbox,
    Anytime,
    Someday,
}

impl StartBucket {
    pub fn from_sqlite(n: i64) -> Self {
        // Things' TMTask.start: 0=Inbox, 1=Anytime, 2=Someday
        match n {
            1 => Self::Anytime,
            2 => Self::Someday,
            _ => Self::Inbox,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TodoSummary {
    pub id: String,
    pub title: String,
    pub status: TaskStatus,
    pub start: StartBucket,
    pub project_id: Option<String>,
    pub area_id: Option<String>,
    pub heading_id: Option<String>,
    pub tags: Vec<String>,
    pub scheduled: Option<String>, // ISO-8601 date, decoded from packed integer
    pub deadline: Option<String>,
    pub creation_date: Option<String>, // ISO-8601 datetime, decoded from REAL unix
    pub modification_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TodoFull {
    #[serde(flatten)]
    pub summary: TodoSummary,
    pub notes: Option<String>,
    pub checklist: Vec<ChecklistItem>,
    pub completion_date: Option<String>,
    pub is_repeating_template: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChecklistItem {
    pub id: String,
    pub title: String,
    pub status: TaskStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Project {
    pub id: String,
    pub title: String,
    pub area_id: Option<String>,
    pub status: TaskStatus,
    pub notes: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Area {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Tag {
    pub id: String,
    pub title: String,
    pub parent_id: Option<String>,
    pub shortcut: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Heading {
    pub id: String,
    pub title: String,
    pub items: Vec<TodoSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProjectFull {
    #[serde(flatten)]
    pub project: Project,
    /// To-dos that live directly under the project (no heading).
    pub items: Vec<TodoSummary>,
    /// Headings, each carrying its own ordered child to-dos.
    pub headings: Vec<Heading>,
    pub completion_date: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WriteOutcome {
    pub id: Option<String>,
    pub action: String,
    pub verified: bool,
    pub dry_run: bool,
    pub latency_ms: u64,
}
