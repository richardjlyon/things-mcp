//! `rmcp` `ServerHandler` implementation. Tools are registered with
//! `#[tool_router]` and each delegates to a `tools::*` function. Outputs are
//! returned as `Json<T>` — `rmcp` serialises and emits the structured payload.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{Implementation, ServerCapabilities, ServerInfo};
use rmcp::{tool, tool_handler, tool_router, ErrorData as McpError, Json, ServerHandler};

use crate::core::types::TodoSummary;
use crate::state::AppState;
use crate::tools::lists::{things_list_inbox, ListInboxArgs};

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
}

#[tool_handler]
impl ServerHandler for ThingsServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("things-mcp", env!("CARGO_PKG_VERSION")))
    }
}
