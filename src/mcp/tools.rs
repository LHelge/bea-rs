use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::{ServerHandler, tool, tool_handler, tool_router};

use crate::service::{self, NewTask, UpdateFields};
use crate::task::Priority;

use super::params::*;
use super::{BeaMcp, ok_json};

#[tool_router]
impl BeaMcp {
    #[tool(description = "List tasks that are ready to work on (open with all dependencies done)")]
    async fn list_ready(
        &self,
        Parameters(params): Parameters<ListReadyParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.with_tasks(|tasks| {
            let ready = service::list_ready(
                &tasks,
                params.tag.as_deref(),
                params.limit.map(|v| v as usize),
                params.epic.as_deref(),
            );
            let eff = service::effective_priorities(&tasks);
            let summaries: Vec<_> = ready.iter().map(|t| t.summary(eff.get(&t.id))).collect();
            ok_json(&summaries)
        })
        .await
    }

    #[tool(description = "List tasks with optional filters")]
    async fn list_all_tasks(
        &self,
        Parameters(params): Parameters<ListTasksFilterParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.with_tasks(|tasks| {
            let filtered = service::list_tasks(
                &tasks,
                params.status,
                params.priority,
                params.tag.as_deref(),
                true, // MCP always shows all statuses unless filtered
                params.epic.as_deref(),
            );
            let eff = service::effective_priorities(&tasks);
            let summaries: Vec<_> = filtered.iter().map(|t| t.summary(eff.get(&t.id))).collect();
            ok_json(&summaries)
        })
        .await
    }

    #[tool(description = "Get full details of a single task")]
    async fn get_task(
        &self,
        Parameters(params): Parameters<TaskIdParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.with_tasks(|tasks| {
            let t = service::get_task(&tasks, &params.id)?;
            let eff = service::effective_priorities(&tasks);
            ok_json(&t.detail(eff.get(&t.id)))
        })
        .await
    }

    #[tool(description = "Create a new task")]
    async fn create_task(
        &self,
        Parameters(params): Parameters<CreateTaskParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.with_tasks(|tasks| {
            let new = NewTask {
                priority: params.priority.unwrap_or(Priority::P2),
                tags: params.tags.unwrap_or_default(),
                depends_on: params.depends_on.unwrap_or_default(),
                parent: params.parent,
                body: params.body.unwrap_or_default(),
                task_type: params.task_type.unwrap_or_default(),
                ..NewTask::new(params.title)
            };
            let t = service::create_task(&self.base, &tasks, new)?;
            ok_json(&t.summary(None))
        })
        .await
    }

    #[tool(description = "Update task fields")]
    async fn update_task(
        &self,
        Parameters(params): Parameters<UpdateTaskParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.with_tasks(|tasks| {
            let fields = UpdateFields {
                status: params.status,
                priority: params.priority,
                tags: params.tags,
                assignee: params.assignee,
                body: params.body,
                title: None, // MCP doesn't support title update currently
            };
            let t = service::update_task(&self.base, &tasks, &params.id, fields)?;
            ok_json(&t.summary(None))
        })
        .await
    }

    #[tool(description = "Start a task (set status to in_progress)")]
    async fn start_task(
        &self,
        Parameters(params): Parameters<TaskIdParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.set_status_tool(&params.id, crate::task::Status::InProgress)
            .await
    }

    #[tool(description = "Complete a task (set status to done)")]
    async fn complete_task(
        &self,
        Parameters(params): Parameters<TaskIdParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.set_status_tool(&params.id, crate::task::Status::Done)
            .await
    }

    #[tool(description = "Cancel a task (set status to cancelled)")]
    async fn cancel_task(
        &self,
        Parameters(params): Parameters<TaskIdParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.set_status_tool(&params.id, crate::task::Status::Cancelled)
            .await
    }

    #[tool(description = "Add a dependency between tasks")]
    async fn add_dependency(
        &self,
        Parameters(params): Parameters<DepParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.with_tasks(|tasks| {
            let t = service::add_dependency(&self.base, &tasks, &params.id, &params.depends_on)?;
            ok_json(&t.summary(None))
        })
        .await
    }

    #[tool(description = "Remove a dependency between tasks")]
    async fn remove_dependency(
        &self,
        Parameters(params): Parameters<DepParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.with_tasks(|tasks| {
            let t = service::remove_dependency(&self.base, &tasks, &params.id, &params.depends_on)?;
            ok_json(&t.summary(None))
        })
        .await
    }

    #[tool(description = "Search tasks by text query")]
    async fn search_tasks(
        &self,
        Parameters(params): Parameters<SearchParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.with_tasks(|tasks| {
            let results = service::search_tasks(&tasks, &params.query, true);
            let summaries: Vec<_> = results.iter().map(|t| t.summary(None)).collect();
            ok_json(&summaries)
        })
        .await
    }

    #[tool(
        description = "Delete cancelled tasks. Set include_done=true to also delete done tasks."
    )]
    async fn prune_tasks(
        &self,
        Parameters(params): Parameters<PruneParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.with_tasks(|tasks| {
            let include_done = params.include_done.unwrap_or(false);
            let deleted = service::prune_tasks(&self.base, &tasks, include_done)?;
            let summaries: Vec<_> = deleted.iter().map(|t| t.summary(None)).collect();
            ok_json(&summaries)
        })
        .await
    }

    #[tool(description = "Permanently delete a task by ID")]
    async fn delete_task(
        &self,
        Parameters(params): Parameters<TaskIdParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.with_tasks(|tasks| {
            let t = service::delete_task(&self.base, &tasks, &params.id)?;
            ok_json(&t.summary(None))
        })
        .await
    }

    #[tool(description = "Get the full dependency graph as an adjacency list")]
    async fn get_graph(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        self.with_tasks(|tasks| {
            let graph = service::build_graph(&tasks);
            ok_json(&graph.adjacency_list())
        })
        .await
    }

    #[tool(description = "List all epics with progress summary")]
    async fn list_epics(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        self.with_tasks(|tasks| {
            let summaries: Vec<_> = service::list_epics(&tasks)
                .iter()
                .map(|t| t.epic_summary(service::epic_progress(&tasks, &t.id)))
                .collect();
            ok_json(&summaries)
        })
        .await
    }
}

impl BeaMcp {
    /// Shared body for the start/complete/cancel status tools.
    async fn set_status_tool(
        &self,
        id: &str,
        status: crate::task::Status,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.with_tasks(|tasks| {
            let t = service::set_status(&self.base, &tasks, id, status)?;
            ok_json(&t.summary(None))
        })
        .await
    }

    pub(super) fn build_tool_router() -> ToolRouter<Self> {
        Self::tool_router()
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

#[cfg(test)]
mod tests {
    use rmcp::handler::server::wrapper::Parameters;
    use rmcp::model::*;

    use crate::store;
    use crate::task::Priority;

    use super::super::BeaMcp;
    use super::super::params::*;

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

    fn extract_text(result: &CallToolResult) -> &str {
        match &result.content[0].raw {
            RawContent::Text(t) => &t.text,
            _ => panic!("expected text content"),
        }
    }

    fn create_params(title: &str) -> CreateTaskParams {
        CreateTaskParams {
            title: title.into(),
            priority: None,
            tags: None,
            depends_on: None,
            parent: None,
            body: None,
            task_type: None,
        }
    }

    #[tokio::test]
    async fn test_tool_create_and_list() {
        let (_tmp, mcp) = setup();
        let result = mcp
            .create_task(Parameters(CreateTaskParams {
                priority: Some(Priority::P1),
                tags: Some(vec!["backend".into()]),
                ..create_params("Test task")
            }))
            .await
            .unwrap();
        let json = extract_json(&result);
        assert_eq!(json["title"], "Test task");
        assert_eq!(json["priority"], "P1");
        let id = json["id"].as_str().unwrap();

        let list = mcp
            .list_all_tasks(Parameters(ListTasksFilterParams {
                status: None,
                priority: None,
                tag: None,
                epic: None,
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
                body: Some("Some body".into()),
                ..create_params("Detail task")
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
            .create_task(Parameters(create_params("Flow task")))
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
            .create_task(Parameters(create_params("First")))
            .await
            .unwrap();
        let id1 = extract_json(&t1)["id"].as_str().unwrap().to_string();

        mcp.create_task(Parameters(CreateTaskParams {
            depends_on: Some(vec![id1.clone()]),
            ..create_params("Second")
        }))
        .await
        .unwrap();

        // Only first should be ready
        let ready = mcp
            .list_ready(Parameters(ListReadyParams {
                limit: None,
                tag: None,
                epic: None,
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
                epic: None,
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
            .create_task(Parameters(create_params("A")))
            .await
            .unwrap();
        let id_a = extract_json(&t1)["id"].as_str().unwrap().to_string();

        let t2 = mcp
            .create_task(Parameters(CreateTaskParams {
                depends_on: Some(vec![id_a.clone()]),
                ..create_params("B")
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
            tags: Some(vec!["auth".into()]),
            ..create_params("Implement OAuth")
        }))
        .await
        .unwrap();
        mcp.create_task(Parameters(create_params("Fix database")))
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
            .create_task(Parameters(create_params("To be deleted")))
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
            .create_task(Parameters(create_params("A")))
            .await
            .unwrap();
        let id_a = extract_json(&t1)["id"].as_str().unwrap().to_string();

        mcp.create_task(Parameters(CreateTaskParams {
            depends_on: Some(vec![id_a]),
            ..create_params("B")
        }))
        .await
        .unwrap();

        let graph = mcp.get_graph().await.unwrap();
        let json = extract_json(&graph);
        assert!(json.is_object());
    }

    #[tokio::test]
    async fn test_tool_create_unknown_parent_is_tool_error() {
        let (_tmp, mcp) = setup();
        let result = mcp
            .create_task(Parameters(CreateTaskParams {
                parent: Some("zzzz".into()),
                ..create_params("Orphan")
            }))
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(true));
        assert!(extract_text(&result).contains("not found"));
    }
}
