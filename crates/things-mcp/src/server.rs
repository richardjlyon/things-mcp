//! `rmcp` `ServerHandler` implementation. Tools are registered with
//! `#[tool_router]` and each delegates to a `tools::*` function. Outputs are
//! returned as `Json<T>` — `rmcp` serialises and emits the structured payload.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{Implementation, ServerCapabilities, ServerInfo};
use rmcp::{tool, tool_handler, tool_router, ErrorData as McpError, Json, ServerHandler};

use crate::core::types::{Area, Project, Tag, TodoSummary};
use crate::state::AppState;
use crate::tools::lists::{
    things_list_anytime, things_list_areas, things_list_inbox, things_list_logbook,
    things_list_projects, things_list_someday, things_list_tags, things_list_today,
    things_list_trash, things_list_upcoming, ListAnytimeArgs, ListAreasArgs,
    ListInboxArgs, ListLogbookArgs, ListProjectsArgs, ListSomedayArgs, ListTagsArgs,
    ListTodayArgs, ListTrashArgs, ListUpcomingArgs,
};

#[derive(Clone)]
pub struct ThingsServer {
    pub state: AppState,
}

#[tool_router]
impl ThingsServer {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    #[tool(
        name = "things_list_inbox",
        description = "Return to-dos in the Things Inbox. Read-only.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn tool_list_inbox(
        &self,
        Parameters(args): Parameters<ListInboxArgs>,
    ) -> Result<Json<Vec<TodoSummary>>, McpError> {
        let state = self.state.clone();
        let rows = things_list_inbox(state, args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(rows))
    }

    #[tool(
        name = "things_list_today",
        description = "Return to-dos scheduled for today (start = Anytime with startDate ≤ today). Read-only.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn tool_list_today(
        &self,
        Parameters(args): Parameters<ListTodayArgs>,
    ) -> Result<Json<Vec<TodoSummary>>, McpError> {
        let rows = things_list_today(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(rows))
    }

    #[tool(
        name = "things_list_upcoming",
        description = "Return scheduled or deadlined to-dos in the future. Read-only.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn tool_list_upcoming(
        &self,
        Parameters(args): Parameters<ListUpcomingArgs>,
    ) -> Result<Json<Vec<TodoSummary>>, McpError> {
        let rows = things_list_upcoming(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(rows))
    }

    #[tool(
        name = "things_list_anytime",
        description = "Return Anytime to-dos (start=Anytime, no scheduled date). Optionally filter by area. Read-only.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn tool_list_anytime(
        &self,
        Parameters(args): Parameters<ListAnytimeArgs>,
    ) -> Result<Json<Vec<TodoSummary>>, McpError> {
        let rows = things_list_anytime(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(rows))
    }

    #[tool(
        name = "things_list_someday",
        description = "Return Someday to-dos (start = Someday). Read-only.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn tool_list_someday(
        &self,
        Parameters(args): Parameters<ListSomedayArgs>,
    ) -> Result<Json<Vec<TodoSummary>>, McpError> {
        let rows = things_list_someday(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(rows))
    }

    #[tool(
        name = "things_list_logbook",
        description = "Return completed or canceled to-dos, newest first. Read-only.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn tool_list_logbook(
        &self,
        Parameters(args): Parameters<ListLogbookArgs>,
    ) -> Result<Json<Vec<TodoSummary>>, McpError> {
        let rows = things_list_logbook(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(rows))
    }

    #[tool(
        name = "things_list_trash",
        description = "Return trashed to-dos, newest first. Read-only.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn tool_list_trash(
        &self,
        Parameters(args): Parameters<ListTrashArgs>,
    ) -> Result<Json<Vec<TodoSummary>>, McpError> {
        let rows = things_list_trash(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(rows))
    }

    #[tool(
        name = "things_list_areas",
        description = "Return all areas, ordered by display index. Read-only.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn tool_list_areas(
        &self,
        Parameters(args): Parameters<ListAreasArgs>,
    ) -> Result<Json<Vec<Area>>, McpError> {
        let rows = things_list_areas(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(rows))
    }

    #[tool(
        name = "things_list_projects",
        description = "Return projects, optionally restricted to a single area and/or a status filter (open/done/all). Read-only.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn tool_list_projects(
        &self,
        Parameters(args): Parameters<ListProjectsArgs>,
    ) -> Result<Json<Vec<Project>>, McpError> {
        let rows = things_list_projects(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(rows))
    }

    #[tool(
        name = "things_list_tags",
        description = "Return all tags. Each carries `parent_id` so callers can rebuild the hierarchy. Read-only.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn tool_list_tags(
        &self,
        Parameters(args): Parameters<ListTagsArgs>,
    ) -> Result<Json<Vec<Tag>>, McpError> {
        let rows = things_list_tags(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(rows))
    }
}

#[tool_handler]
impl ServerHandler for ThingsServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("things-mcp", env!("CARGO_PKG_VERSION")))
    }
}
