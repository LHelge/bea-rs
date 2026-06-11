mod params;
mod tools;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rmcp::ServiceExt;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::model::*;

use crate::error::Error;
use crate::store;
use crate::task::Task;

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

    /// Load all tasks and run a tool body against them.
    ///
    /// Boundary: domain errors become MCP tool-level errors (isError=true),
    /// not JSON-RPC protocol errors.
    async fn with_tasks<F>(&self, f: F) -> Result<CallToolResult, rmcp::ErrorData>
    where
        F: FnOnce(HashMap<String, Task>) -> Result<CallToolResult, Error>,
    {
        let result = match store::load_all(&self.base).await {
            Ok(tasks) => f(tasks),
            Err(e) => Err(e),
        };
        Ok(result.unwrap_or_else(|e| CallToolResult::error(vec![Content::text(e.to_string())])))
    }
}

fn ok_json<T: serde::Serialize>(value: &T) -> Result<CallToolResult, Error> {
    let text = serde_json::to_string(value)?;
    Ok(CallToolResult::success(vec![Content::text(text)]))
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
