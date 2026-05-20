//! `rmcp` `ServerHandler` implementation. Tools are registered with
//! `#[tool_router]` and each delegates to a `tools::*` function. Outputs are
//! returned as `Json<T>` — `rmcp` serialises and emits the structured payload.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{Implementation, ServerCapabilities, ServerInfo};
use rmcp::{tool, tool_handler, tool_router, ErrorData as McpError, Json, ServerHandler};

use crate::core::types::{Area, Project, ProjectFull, Tag, TodoFull, TodoSummary};
use crate::tools::todos::{
    things_add_todo, things_cancel_todo, things_complete_todo, things_get_todo,
    things_move_todo, things_update_todo,
    AddTodoArgs, GetTodoArgs, MoveTodoArgs, StatusChangeArgs, UpdateTodoArgs,
};
use crate::core::writer::outcome::WriteOutcome;
use crate::tools::projects::{
    things_add_project, things_get_project, things_update_project,
    AddProjectArgs, GetProjectArgs, UpdateProjectArgs,
};
use crate::tools::bulk::{things_bulk_json, BulkJsonArgs};
use crate::tools::search::{things_search, SearchArgs};
use crate::state::AppState;
use crate::tools::lists::{
    things_list_anytime, things_list_areas, things_list_by_tag, things_list_inbox,
    things_list_logbook, things_list_projects, things_list_someday, things_list_tags,
    things_list_today, things_list_trash, things_list_upcoming, ListAnytimeArgs,
    ListAreasArgs, ListByTagArgs, ListInboxArgs, ListLogbookArgs, ListProjectsArgs,
    ListSomedayArgs, ListTagsArgs, ListTodayArgs, ListTrashArgs, ListUpcomingArgs,
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

    #[tool(
        name = "things_list_by_tag",
        description = "Return to-dos carrying a given tag. `tag` accepts the tag's title or UUID. With `recurse=true` (default), descendants of the tag are included. Read-only.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn tool_list_by_tag(
        &self,
        Parameters(args): Parameters<ListByTagArgs>,
    ) -> Result<Json<Vec<TodoSummary>>, McpError> {
        let rows = things_list_by_tag(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(rows))
    }

    #[tool(
        name = "things_get_todo",
        description = "Return a single to-do with notes, checklist, tags, and a repeating-template flag. Returns null if not found. Read-only.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn tool_get_todo(
        &self,
        Parameters(args): Parameters<GetTodoArgs>,
    ) -> Result<Json<Option<TodoFull>>, McpError> {
        let res = things_get_todo(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(res))
    }

    #[tool(
        name = "things_get_project",
        description = "Return a single project with its child to-dos and headings. Returns null if not found. Read-only.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn tool_get_project(
        &self,
        Parameters(args): Parameters<GetProjectArgs>,
    ) -> Result<Json<Option<ProjectFull>>, McpError> {
        let res = things_get_project(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(res))
    }

    #[tool(
        name = "things_search",
        description = "Search to-dos by free text (title + notes) and structured filters (tags, area, project, status, deadline range, scheduled range). Read-only.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn tool_search(
        &self,
        Parameters(args): Parameters<SearchArgs>,
    ) -> Result<Json<Vec<TodoSummary>>, McpError> {
        let rows = things_search(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(rows))
    }

    #[tool(
        name = "things_add_todo",
        description = "Create a new to-do in Things. Returns a WriteOutcome with the new id once verified by polling the SQLite reader. Requires `title`; all other fields are optional. Open-world: side-effects the live Things app via the JSON URL scheme.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn tool_add_todo(
        &self,
        Parameters(args): Parameters<AddTodoArgs>,
    ) -> Result<Json<WriteOutcome>, McpError> {
        let out = things_add_todo(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(out))
    }

    #[tool(
        name = "things_add_project",
        description = "Create a new project in Things, optionally with initial headings nested inside. Returns a WriteOutcome with the new id once verified.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn tool_add_project(
        &self,
        Parameters(args): Parameters<AddProjectArgs>,
    ) -> Result<Json<WriteOutcome>, McpError> {
        let out = things_add_project(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(out))
    }

    #[tool(
        name = "things_update_todo",
        description = "Update an existing to-do's title, notes, scheduling, tags, list, or status. Only populated fields are sent. Requires the Things auth-token.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn tool_update_todo(
        &self,
        Parameters(args): Parameters<UpdateTodoArgs>,
    ) -> Result<Json<WriteOutcome>, McpError> {
        let out = things_update_todo(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(out))
    }

    #[tool(
        name = "things_update_project",
        description = "Update an existing project's title, notes, scheduling, tags, parent area, or status. Only populated fields are sent. Requires the Things auth-token.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn tool_update_project(
        &self,
        Parameters(args): Parameters<UpdateProjectArgs>,
    ) -> Result<Json<WriteOutcome>, McpError> {
        let out = things_update_project(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(out))
    }

    #[tool(
        name = "things_complete_todo",
        description = "Mark a to-do as completed. Idempotent: re-completing has no further effect. Requires the Things auth-token.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    async fn tool_complete_todo(
        &self,
        Parameters(args): Parameters<StatusChangeArgs>,
    ) -> Result<Json<WriteOutcome>, McpError> {
        let out = things_complete_todo(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(out))
    }

    #[tool(
        name = "things_cancel_todo",
        description = "Mark a to-do as canceled (distinct from completed in Things). Idempotent. Requires the Things auth-token.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    async fn tool_cancel_todo(
        &self,
        Parameters(args): Parameters<StatusChangeArgs>,
    ) -> Result<Json<WriteOutcome>, McpError> {
        let out = things_cancel_todo(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(out))
    }

    #[tool(
        name = "things_move_todo",
        description = "Move a to-do under a project, area, or to the Inbox (when list_id is omitted). Requires the Things auth-token.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn tool_move_todo(
        &self,
        Parameters(args): Parameters<MoveTodoArgs>,
    ) -> Result<Json<WriteOutcome>, McpError> {
        let out = things_move_todo(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(out))
    }

    #[tool(
        name = "things_bulk_json",
        description = "Power tool: send a raw array of Things JSON URL scheme operation objects. Max 250 elements. No per-element verification — WriteOutcome.verified is always false. Use individual tools when verification matters.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn tool_bulk_json(
        &self,
        Parameters(args): Parameters<BulkJsonArgs>,
    ) -> Result<Json<WriteOutcome>, McpError> {
        let out = things_bulk_json(self.state.clone(), args)
            .await
            .map_err(|e| McpError::internal_error(format!("{e:#}"), None))?;
        Ok(Json(out))
    }
}

#[tool_handler]
impl ServerHandler for ThingsServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("things-mcp", env!("CARGO_PKG_VERSION")))
    }
}
