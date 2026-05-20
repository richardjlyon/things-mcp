//! Read tools that surface a Things list view.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::core::reader::queries::{list_inbox, list_today, ListInboxParams, ListTodayParams};
use crate::core::reader::queries::{list_upcoming, ListUpcomingParams};
use crate::core::reader::queries::{list_anytime, ListAnytimeParams};
use crate::core::reader::queries::{list_someday, ListSomedayParams};
use crate::core::reader::queries::{list_logbook, ListLogbookParams};
use crate::core::reader::queries::{list_trash, ListTrashParams};
use crate::core::types::TodoSummary;
use crate::state::AppState;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct ListInboxArgs {
    /// Cap on returned rows. Defaults to 200.
    #[serde(default)]
    pub limit: Option<u32>,
    /// If true, completed inbox to-dos are also returned. Defaults to false.
    #[serde(default)]
    pub include_completed: Option<bool>,
}

pub async fn things_list_inbox(
    state: AppState,
    args: ListInboxArgs,
) -> anyhow::Result<Vec<TodoSummary>> {
    let params = ListInboxParams {
        include_completed: args.include_completed.unwrap_or(false),
        limit: args.limit.unwrap_or(200),
    };
    let rows = list_inbox(&state.pool, params).await?;
    Ok(rows)
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct ListTodayArgs {
    /// Cap on returned rows. Defaults to 200.
    #[serde(default)]
    pub limit: Option<u32>,
}

pub async fn things_list_today(
    state: AppState,
    args: ListTodayArgs,
) -> anyhow::Result<Vec<TodoSummary>> {
    let params = ListTodayParams {
        limit: args.limit.unwrap_or(200),
    };
    let rows = list_today(&state.pool, params).await?;
    Ok(rows)
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct ListUpcomingArgs {
    /// Lower bound (exclusive) as `YYYY-MM-DD`. Defaults to today.
    #[serde(default)]
    pub from: Option<String>,
    /// Upper bound (inclusive) as `YYYY-MM-DD`. If omitted, no upper bound.
    #[serde(default)]
    pub to: Option<String>,
    /// Cap on returned rows. Defaults to 200.
    #[serde(default)]
    pub limit: Option<u32>,
}

pub async fn things_list_upcoming(
    state: AppState,
    args: ListUpcomingArgs,
) -> anyhow::Result<Vec<TodoSummary>> {
    let params = ListUpcomingParams {
        from_iso: args.from,
        to_iso: args.to,
        limit: args.limit.unwrap_or(200),
    };
    let rows = list_upcoming(&state.pool, params).await?;
    Ok(rows)
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct ListAnytimeArgs {
    /// Restrict to to-dos belonging to a specific area (directly or via project). Optional.
    #[serde(default)]
    pub area_id: Option<String>,
    /// Cap on returned rows. Defaults to 200.
    #[serde(default)]
    pub limit: Option<u32>,
}

pub async fn things_list_anytime(
    state: AppState,
    args: ListAnytimeArgs,
) -> anyhow::Result<Vec<TodoSummary>> {
    let params = ListAnytimeParams {
        area_id: args.area_id,
        limit: args.limit.unwrap_or(200),
    };
    let rows = list_anytime(&state.pool, params).await?;
    Ok(rows)
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct ListSomedayArgs {
    /// Cap on returned rows. Defaults to 200.
    #[serde(default)]
    pub limit: Option<u32>,
}

pub async fn things_list_someday(
    state: AppState,
    args: ListSomedayArgs,
) -> anyhow::Result<Vec<TodoSummary>> {
    let params = ListSomedayParams {
        limit: args.limit.unwrap_or(200),
    };
    let rows = list_someday(&state.pool, params).await?;
    Ok(rows)
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct ListLogbookArgs {
    /// Lower bound on completion date as `YYYY-MM-DD` (inclusive). Optional.
    #[serde(default)]
    pub from: Option<String>,
    /// Upper bound on completion date as `YYYY-MM-DD` (inclusive — end-of-day). Optional.
    #[serde(default)]
    pub to: Option<String>,
    /// Cap on returned rows. Defaults to 100.
    #[serde(default)]
    pub limit: Option<u32>,
}

pub async fn things_list_logbook(
    state: AppState,
    args: ListLogbookArgs,
) -> anyhow::Result<Vec<TodoSummary>> {
    let params = ListLogbookParams {
        from_iso: args.from,
        to_iso: args.to,
        limit: args.limit.unwrap_or(100),
    };
    let rows = list_logbook(&state.pool, params).await?;
    Ok(rows)
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct ListTrashArgs {
    /// Cap on returned rows. Defaults to 100.
    #[serde(default)]
    pub limit: Option<u32>,
}

pub async fn things_list_trash(
    state: AppState,
    args: ListTrashArgs,
) -> anyhow::Result<Vec<TodoSummary>> {
    let params = ListTrashParams {
        limit: args.limit.unwrap_or(100),
    };
    let rows = list_trash(&state.pool, params).await?;
    Ok(rows)
}
