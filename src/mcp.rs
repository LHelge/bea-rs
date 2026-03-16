use std::collections::HashSet;
use std::path::{Path, PathBuf};

use chrono::Utc;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::schemars;
use rmcp::{ServerHandler, ServiceExt, tool, tool_handler, tool_router};
use serde::Deserialize;

use crate::error::Error;
use crate::graph::Graph;
use crate::store;
use crate::task::{self, Priority, Status, Task};

#[derive(Clone)]
pub struct BeaMcp {
    base: PathBuf,
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

impl BeaMcp {
    pub fn new(base: PathBuf) -> Self {
        Self {
            base,
            tool_router: Self::tool_router(),
        }
    }
}

// Parameter structs for tool inputs

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListReadyParams {
    /// Max number of results
    limit: Option<u64>,
    /// Filter by tag
    tag: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListTasksFilterParams {
    /// Filter by status (open, in_progress, done, blocked, cancelled)
    status: Option<String>,
    /// Filter by priority (P0, P1, P2, P3)
    priority: Option<String>,
    /// Filter by tag
    tag: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TaskIdParams {
    /// Task ID
    id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateTaskParams {
    /// Task title
    title: String,
    /// Priority (P0, P1, P2, P3)
    priority: Option<String>,
    /// Tags
    tags: Option<Vec<String>>,
    /// IDs of tasks this depends on
    depends_on: Option<Vec<String>>,
    /// Parent task ID
    parent: Option<String>,
    /// Task body (markdown)
    body: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateTaskParams {
    /// Task ID
    id: String,
    /// New status (open, in_progress, done, blocked, cancelled)
    status: Option<String>,
    /// New priority (P0, P1, P2, P3)
    priority: Option<String>,
    /// New tags (replaces existing)
    tags: Option<Vec<String>>,
    /// New assignee
    assignee: Option<String>,
    /// New body (markdown)
    body: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DepParams {
    /// Task that will depend on another
    id: String,
    /// Task to depend on
    depends_on: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchParams {
    /// Search query
    query: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PruneParams {
    /// Also delete done tasks (default: only cancelled)
    include_done: Option<bool>,
}

fn task_summary(t: &Task) -> serde_json::Value {
    serde_json::json!({
        "id": t.id,
        "title": t.title,
        "status": t.status,
        "priority": t.priority,
        "tags": t.tags,
    })
}

fn ok_json(value: serde_json::Value) -> Result<CallToolResult, rmcp::ErrorData> {
    let text = serde_json::to_string(&value)
        .map_err(|e| rmcp::ErrorData::internal_error(format!("JSON error: {e}"), None))?;
    Ok(CallToolResult::success(vec![Content::text(text)]))
}

fn err_result(e: Error) -> Result<CallToolResult, rmcp::ErrorData> {
    Ok(CallToolResult::error(vec![Content::text(e.to_string())]))
}

/// Map our domain errors to MCP results. Tool-level errors become isError=true results,
/// not JSON-RPC errors (per MCP convention).
macro_rules! try_tool {
    ($expr:expr) => {
        match $expr {
            Ok(v) => v,
            Err(e) => return err_result(e),
        }
    };
}

#[tool_router]
impl BeaMcp {
    #[tool(description = "List tasks that are ready to work on (open with all dependencies done)")]
    async fn list_ready(
        &self,
        Parameters(params): Parameters<ListReadyParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let tasks = try_tool!(store::load_all(&self.base).await);
        let graph = Graph::build(&tasks);
        let limit = params.limit.map(|v| v as usize);
        let ready = graph.ready(&tasks, params.tag.as_deref(), limit);
        let summaries: Vec<_> = ready.iter().map(|t| task_summary(t)).collect();
        ok_json(serde_json::json!(summaries))
    }

    #[tool(description = "List tasks with optional filters")]
    async fn list_all_tasks(
        &self,
        Parameters(params): Parameters<ListTasksFilterParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let tasks = try_tool!(store::load_all(&self.base).await);
        let status: Option<Status> = params.status.as_deref().and_then(|s| s.parse().ok());
        let priority: Option<Priority> = params.priority.as_deref().and_then(|s| s.parse().ok());

        let mut filtered: Vec<&Task> = tasks
            .values()
            .filter(|t| status.as_ref().is_none_or(|s| t.status == *s))
            .filter(|t| priority.as_ref().is_none_or(|p| t.priority == *p))
            .filter(|t| {
                params
                    .tag
                    .as_ref()
                    .is_none_or(|tag| t.tags.iter().any(|tt| tt == tag))
            })
            .collect();
        filtered.sort_by(|a, b| a.priority.cmp(&b.priority).then(a.created.cmp(&b.created)));

        let summaries: Vec<_> = filtered.iter().map(|t| task_summary(t)).collect();
        ok_json(serde_json::json!(summaries))
    }

    #[tool(description = "Get full details of a single task")]
    async fn get_task(
        &self,
        Parameters(params): Parameters<TaskIdParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let t = try_tool!(store::load_one(&self.base, &params.id));
        ok_json(serde_json::json!({
            "id": t.id,
            "title": t.title,
            "status": t.status,
            "priority": t.priority,
            "tags": t.tags,
            "depends_on": t.depends_on,
            "parent": t.parent,
            "assignee": t.assignee,
            "created": t.created,
            "updated": t.updated,
            "body": t.body,
        }))
    }

    #[tool(description = "Create a new task")]
    async fn create_task(
        &self,
        Parameters(params): Parameters<CreateTaskParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let priority: Priority = params
            .priority
            .as_deref()
            .and_then(|s| s.parse().ok())
            .unwrap_or(Priority::P2);

        let tasks = try_tool!(store::load_all(&self.base).await);
        let existing_ids: HashSet<String> = tasks.keys().cloned().collect();
        let id = task::generate_id(&existing_ids);

        let mut t = Task::new(id, params.title, priority);
        t.tags = params.tags.unwrap_or_default();
        t.depends_on = params.depends_on.unwrap_or_default();
        t.parent = params.parent;
        t.body = params.body.unwrap_or_default();

        try_tool!(store::save(&self.base, &t));
        ok_json(task_summary(&t))
    }

    #[tool(description = "Update task fields")]
    async fn update_task(
        &self,
        Parameters(params): Parameters<UpdateTaskParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let mut t = try_tool!(store::load_one(&self.base, &params.id));

        if let Some(s) = params
            .status
            .as_deref()
            .and_then(|s| s.parse::<Status>().ok())
        {
            t.status = s;
        }
        if let Some(p) = params
            .priority
            .as_deref()
            .and_then(|s| s.parse::<Priority>().ok())
        {
            t.priority = p;
        }
        if let Some(tags) = params.tags {
            t.tags = tags;
        }
        if let Some(a) = params.assignee {
            t.assignee = a;
        }
        if let Some(b) = params.body {
            t.body = b;
        }

        t.updated = Utc::now();
        try_tool!(store::save(&self.base, &t));
        ok_json(task_summary(&t))
    }

    #[tool(description = "Start a task (set status to in_progress)")]
    async fn start_task(
        &self,
        Parameters(params): Parameters<TaskIdParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let mut t = try_tool!(store::load_one(&self.base, &params.id));
        t.status = Status::InProgress;
        t.updated = Utc::now();
        try_tool!(store::save(&self.base, &t));
        ok_json(task_summary(&t))
    }

    #[tool(description = "Complete a task (set status to done)")]
    async fn complete_task(
        &self,
        Parameters(params): Parameters<TaskIdParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let mut t = try_tool!(store::load_one(&self.base, &params.id));
        t.status = Status::Done;
        t.updated = Utc::now();
        try_tool!(store::save(&self.base, &t));
        ok_json(task_summary(&t))
    }

    #[tool(description = "Add a dependency between tasks")]
    async fn add_dependency(
        &self,
        Parameters(params): Parameters<DepParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        // Verify dependency target exists
        try_tool!(store::load_one(&self.base, &params.depends_on));
        let mut t = try_tool!(store::load_one(&self.base, &params.id));

        // Check for cycles
        let tasks = try_tool!(store::load_all(&self.base).await);
        let graph = Graph::build(&tasks);
        if graph.would_cycle(&params.id, &params.depends_on) {
            return err_result(Error::CycleDetected {
                from: params.id,
                to: params.depends_on,
            });
        }

        if !t.depends_on.contains(&params.depends_on) {
            t.depends_on.push(params.depends_on);
            t.updated = Utc::now();
            try_tool!(store::save(&self.base, &t));
        }

        ok_json(task_summary(&t))
    }

    #[tool(description = "Remove a dependency between tasks")]
    async fn remove_dependency(
        &self,
        Parameters(params): Parameters<DepParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let mut t = try_tool!(store::load_one(&self.base, &params.id));
        t.depends_on.retain(|d| d != &params.depends_on);
        t.updated = Utc::now();
        try_tool!(store::save(&self.base, &t));
        ok_json(task_summary(&t))
    }

    #[tool(description = "Search tasks by text query")]
    async fn search_tasks(
        &self,
        Parameters(params): Parameters<SearchParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let tasks = try_tool!(store::load_all(&self.base).await);
        let query_lower = params.query.to_lowercase();
        let mut results: Vec<&Task> = tasks
            .values()
            .filter(|t| {
                t.title.to_lowercase().contains(&query_lower)
                    || t.body.to_lowercase().contains(&query_lower)
                    || t.tags
                        .iter()
                        .any(|tag| tag.to_lowercase().contains(&query_lower))
                    || t.id.contains(&query_lower)
            })
            .collect();
        results.sort_by(|a, b| a.priority.cmp(&b.priority).then(a.created.cmp(&b.created)));

        let summaries: Vec<_> = results.iter().map(|t| task_summary(t)).collect();
        ok_json(serde_json::json!(summaries))
    }

    #[tool(description = "Cancel a task (set status to cancelled)")]
    async fn cancel_task(
        &self,
        Parameters(params): Parameters<TaskIdParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let mut t = try_tool!(store::load_one(&self.base, &params.id));
        t.status = Status::Cancelled;
        t.updated = Utc::now();
        try_tool!(store::save(&self.base, &t));
        ok_json(task_summary(&t))
    }

    #[tool(
        description = "Delete cancelled tasks. Set include_done=true to also delete done tasks."
    )]
    async fn prune_tasks(
        &self,
        Parameters(params): Parameters<PruneParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let tasks = try_tool!(store::load_all(&self.base).await);
        let include_done = params.include_done.unwrap_or(false);
        let to_delete: Vec<&Task> = tasks
            .values()
            .filter(|t| t.status == Status::Cancelled || (include_done && t.status == Status::Done))
            .collect();

        let summaries: Vec<_> = to_delete.iter().map(|t| task_summary(t)).collect();
        for t in &to_delete {
            try_tool!(store::delete(&self.base, &t.id));
        }
        ok_json(serde_json::json!(summaries))
    }

    #[tool(description = "Permanently delete a task by ID")]
    async fn delete_task(
        &self,
        Parameters(params): Parameters<TaskIdParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let t = try_tool!(store::load_one(&self.base, &params.id));
        try_tool!(store::delete(&self.base, &params.id));
        ok_json(task_summary(&t))
    }

    #[tool(description = "Get the full dependency graph as an adjacency list")]
    async fn get_graph(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let tasks = try_tool!(store::load_all(&self.base).await);
        let graph = Graph::build(&tasks);
        let adj = graph.adjacency_list();
        ok_json(serde_json::json!(adj))
    }
}

#[tool_handler]
impl ServerHandler for BeaMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("bears", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "bears is a file-based task tracker. Use tools to manage tasks and dependencies."
                    .to_string(),
            )
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, BeaMcp) {
        let tmp = TempDir::new().unwrap();
        store::init(tmp.path()).unwrap();
        let mcp = BeaMcp::new(tmp.path().to_path_buf());
        (tmp, mcp)
    }

    fn extract_json(result: &CallToolResult) -> serde_json::Value {
        let text = match &result.content[0].raw {
            RawContent::Text(t) => &t.text,
            _ => panic!("expected text content"),
        };
        serde_json::from_str(text).unwrap()
    }

    #[tokio::test]
    async fn test_tool_create_and_list() {
        let (_tmp, mcp) = setup();
        let result = mcp
            .create_task(Parameters(CreateTaskParams {
                title: "Test task".into(),
                priority: Some("P1".into()),
                tags: Some(vec!["backend".into()]),
                depends_on: None,
                parent: None,
                body: None,
            }))
            .await
            .unwrap();
        let json = extract_json(&result);
        assert_eq!(json["title"], "Test task");
        let id = json["id"].as_str().unwrap();

        let list = mcp
            .list_all_tasks(Parameters(ListTasksFilterParams {
                status: None,
                priority: None,
                tag: None,
            }))
            .await
            .unwrap();
        let arr = extract_json(&list);
        let arr = arr.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"], id);
    }

    #[tokio::test]
    async fn test_tool_get_task() {
        let (_tmp, mcp) = setup();
        let result = mcp
            .create_task(Parameters(CreateTaskParams {
                title: "Detail task".into(),
                priority: None,
                tags: None,
                depends_on: None,
                parent: None,
                body: Some("Some body".into()),
            }))
            .await
            .unwrap();
        let id = extract_json(&result)["id"].as_str().unwrap().to_string();

        let detail = mcp.get_task(Parameters(TaskIdParams { id })).await.unwrap();
        let json = extract_json(&detail);
        assert_eq!(json["title"], "Detail task");
        assert_eq!(json["body"], "Some body");
    }

    #[tokio::test]
    async fn test_tool_start_complete() {
        let (_tmp, mcp) = setup();
        let result = mcp
            .create_task(Parameters(CreateTaskParams {
                title: "Flow task".into(),
                priority: None,
                tags: None,
                depends_on: None,
                parent: None,
                body: None,
            }))
            .await
            .unwrap();
        let id = extract_json(&result)["id"].as_str().unwrap().to_string();

        let started = mcp
            .start_task(Parameters(TaskIdParams { id: id.clone() }))
            .await
            .unwrap();
        assert_eq!(extract_json(&started)["status"], "in_progress");

        let completed = mcp
            .complete_task(Parameters(TaskIdParams { id }))
            .await
            .unwrap();
        assert_eq!(extract_json(&completed)["status"], "done");
    }

    #[tokio::test]
    async fn test_tool_ready() {
        let (_tmp, mcp) = setup();
        let t1 = mcp
            .create_task(Parameters(CreateTaskParams {
                title: "First".into(),
                priority: None,
                tags: None,
                depends_on: None,
                parent: None,
                body: None,
            }))
            .await
            .unwrap();
        let id1 = extract_json(&t1)["id"].as_str().unwrap().to_string();

        mcp.create_task(Parameters(CreateTaskParams {
            title: "Second".into(),
            priority: None,
            tags: None,
            depends_on: Some(vec![id1.clone()]),
            parent: None,
            body: None,
        }))
        .await
        .unwrap();

        // Only first should be ready
        let ready = mcp
            .list_ready(Parameters(ListReadyParams {
                limit: None,
                tag: None,
            }))
            .await
            .unwrap();
        let arr = extract_json(&ready);
        let arr = arr.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["title"], "First");

        // Complete first
        mcp.complete_task(Parameters(TaskIdParams { id: id1 }))
            .await
            .unwrap();

        // Now second should be ready
        let ready = mcp
            .list_ready(Parameters(ListReadyParams {
                limit: None,
                tag: None,
            }))
            .await
            .unwrap();
        let arr = extract_json(&ready);
        let arr = arr.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["title"], "Second");
    }

    #[tokio::test]
    async fn test_tool_dependency_cycle() {
        let (_tmp, mcp) = setup();
        let t1 = mcp
            .create_task(Parameters(CreateTaskParams {
                title: "A".into(),
                priority: None,
                tags: None,
                depends_on: None,
                parent: None,
                body: None,
            }))
            .await
            .unwrap();
        let id_a = extract_json(&t1)["id"].as_str().unwrap().to_string();

        let t2 = mcp
            .create_task(Parameters(CreateTaskParams {
                title: "B".into(),
                priority: None,
                tags: None,
                depends_on: Some(vec![id_a.clone()]),
                parent: None,
                body: None,
            }))
            .await
            .unwrap();
        let id_b = extract_json(&t2)["id"].as_str().unwrap().to_string();

        let result = mcp
            .add_dependency(Parameters(DepParams {
                id: id_a,
                depends_on: id_b,
            }))
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(true));
    }

    #[tokio::test]
    async fn test_tool_search() {
        let (_tmp, mcp) = setup();
        mcp.create_task(Parameters(CreateTaskParams {
            title: "Implement OAuth".into(),
            priority: None,
            tags: Some(vec!["auth".into()]),
            depends_on: None,
            parent: None,
            body: None,
        }))
        .await
        .unwrap();
        mcp.create_task(Parameters(CreateTaskParams {
            title: "Fix database".into(),
            priority: None,
            tags: None,
            depends_on: None,
            parent: None,
            body: None,
        }))
        .await
        .unwrap();

        let results = mcp
            .search_tasks(Parameters(SearchParams {
                query: "OAuth".into(),
            }))
            .await
            .unwrap();
        let arr = extract_json(&results);
        let arr = arr.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["title"], "Implement OAuth");
    }

    #[tokio::test]
    async fn test_tool_delete_task() {
        let (_tmp, mcp) = setup();
        let result = mcp
            .create_task(Parameters(CreateTaskParams {
                title: "To be deleted".into(),
                priority: None,
                tags: None,
                depends_on: None,
                parent: None,
                body: None,
            }))
            .await
            .unwrap();
        let id = extract_json(&result)["id"].as_str().unwrap().to_string();

        let deleted = mcp
            .delete_task(Parameters(TaskIdParams { id: id.clone() }))
            .await
            .unwrap();
        assert_eq!(extract_json(&deleted)["id"], id);

        // Should no longer be findable
        let not_found = mcp.get_task(Parameters(TaskIdParams { id })).await.unwrap();
        assert_eq!(not_found.is_error, Some(true));
    }

    #[tokio::test]
    async fn test_tool_graph() {
        let (_tmp, mcp) = setup();
        let t1 = mcp
            .create_task(Parameters(CreateTaskParams {
                title: "A".into(),
                priority: None,
                tags: None,
                depends_on: None,
                parent: None,
                body: None,
            }))
            .await
            .unwrap();
        let id_a = extract_json(&t1)["id"].as_str().unwrap().to_string();

        mcp.create_task(Parameters(CreateTaskParams {
            title: "B".into(),
            priority: None,
            tags: None,
            depends_on: Some(vec![id_a]),
            parent: None,
            body: None,
        }))
        .await
        .unwrap();

        let graph = mcp.get_graph().await.unwrap();
        let json = extract_json(&graph);
        assert!(json.is_object());
    }
}
