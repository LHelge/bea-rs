mod params;
mod tools;

use std::path::{Path, PathBuf};

use rmcp::ServiceExt;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::model::*;

use crate::error::Error;
use crate::task::{Priority, Status};

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
}

fn ok_json(value: serde_json::Value) -> Result<CallToolResult, Error> {
    let text = serde_json::to_string(&value)?;
    Ok(CallToolResult::success(vec![Content::text(text)]))
}

/// Boundary: convert domain errors into MCP tool-level errors (isError=true),
/// not JSON-RPC protocol errors.
fn tool_ok(r: Result<CallToolResult, Error>) -> Result<CallToolResult, rmcp::ErrorData> {
    Ok(r.unwrap_or_else(|e| CallToolResult::error(vec![Content::text(e.to_string())])))
}

fn parse_status(s: &str) -> Result<Status, Error> {
    s.parse().map_err(|_| Error::InvalidFilter {
        field: "status".into(),
        value: s.into(),
        expected: "open, in_progress, done, blocked, cancelled".into(),
    })
}

fn parse_priority(s: &str) -> Result<Priority, Error> {
    s.parse().map_err(|_| Error::InvalidFilter {
        field: "priority".into(),
        value: s.into(),
        expected: "P0, P1, P2, P3".into(),
    })
}

pub async fn run(base: &Path) -> crate::error::Result<()> {
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
