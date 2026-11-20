mod params;
mod tools;

use std::path::{Path, PathBuf};

use rmcp::ServiceExt;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::model::{CallToolResult, ContentBlock};

use bears::error::Error;

#[derive(Clone)]
pub struct BeaMcp {
    base: PathBuf,
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

impl BeaMcp {
    pub fn new(base: PathBuf) -> Self {
        let tool_router = Self::build_tool_router();
        Self { base, tool_router }
    }

    /// Resolve an `epic` parameter (an ID or a prefix) to the epic's ID.
    async fn resolve_epic(
        &self,
        tasks: &std::collections::HashMap<String, bears::Task>,
        epic: Option<&str>,
    ) -> Result<Option<String>, Error> {
        match epic {
            Some(e) => Ok(Some(
                bears::service::resolve_epic_filter(&self.base, tasks, e).await?,
            )),
            None => Ok(None),
        }
    }
}

fn ok_json(value: serde_json::Value) -> Result<CallToolResult, Error> {
    let text = serde_json::to_string(&value)?;
    Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
}

/// Boundary: convert domain errors into MCP tool-level errors (isError=true),
/// not JSON-RPC protocol errors. Invalid enum values (status, priority, type)
/// never reach the tool body — they are rejected at the schema/deserialize
/// layer because the parameter structs use the typed enums directly; rmcp
/// surfaces those as tool-level errors too.
///
/// Errors an agent cannot recover from with another tool call get a
/// remediation appended here, since the library's messages carry none.
fn tool_ok(r: Result<CallToolResult, Error>) -> Result<CallToolResult, rmcp::ErrorData> {
    Ok(r.unwrap_or_else(|e| {
        let mut text = e.to_string();
        if let Some(hint) = hint_for(&e) {
            text.push_str(". ");
            text.push_str(hint);
        }
        CallToolResult::error(vec![ContentBlock::text(text)])
    }))
}

/// MCP-specific remediation for an error, appended to the tool error text.
fn hint_for(e: &Error) -> Option<&'static str> {
    match e {
        Error::LegacyStore { .. } => Some(
            "No bears tool works until the store is converted: the user (or you, through a \
             shell) must run `bea migrate` once in the project root; `bea migrate --dry-run` \
             previews which tasks get renamed",
        ),
        _ => None,
    }
}

pub async fn run(base: &Path) -> bears::error::Result<()> {
    let server = BeaMcp::new(base.to_path_buf());
    let service = server
        .serve(rmcp::transport::stdio())
        .await
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    service
        .waiting()
        .await
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(())
}
