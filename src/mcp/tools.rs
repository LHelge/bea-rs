use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::{ServerHandler, tool, tool_handler, tool_router};

use crate::error::Error;
use crate::service;
use crate::store;
use crate::task::{self, Priority, Status, Task};

use super::params::*;
use super::{BeaMcp, ok_json, tool_ok};

#[tool_router]
impl BeaMcp {
    #[tool(description = "List tasks that are ready to work on (open with all dependencies done)")]
    async fn list_ready(
        &self,
        Parameters(params): Parameters<ListReadyParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tool_ok(
            async {
                let tasks = store::load_all(&self.base).await?;
                let limit = params.limit.map(|v| v as usize);
                let ready = service::list_ready(
                    &tasks,
                    params.tag.as_deref(),
                    limit,
                    params.epic.as_deref(),
                );
                let eff = service::effective_priorities(&tasks);
                let summaries: Vec<_> = ready.iter().map(|t| t.summary(eff.get(&t.id))).collect();
                ok_json(serde_json::json!(summaries))
            }
            .await,
        )
    }

    #[tool(description = "List tasks with optional filters")]
    async fn list_all_tasks(
        &self,
        Parameters(params): Parameters<ListTasksFilterParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tool_ok(
            async {
                let tasks = store::load_all(&self.base).await?;
                // active_only=true → include_all=false (hide done/cancelled)
                let include_all = !params.active_only.unwrap_or(false);
                let mut filtered = service::list_tasks(
                    &tasks,
                    params.status,
                    params.priority,
                    params.tag.as_deref(),
                    include_all,
                    params.epic.as_deref(),
                );
                if let Some(limit) = params.limit {
                    filtered.truncate(limit as usize);
                }
                let eff = service::effective_priorities(&tasks);
                let summaries: Vec<_> =
                    filtered.iter().map(|t| t.summary(eff.get(&t.id))).collect();
                ok_json(serde_json::json!(summaries))
            }
            .await,
        )
    }

    #[tool(description = "Get full details of a single task. If the id isn't an \
                       active task, falls back to the archive; an archived \
                       result is marked with \"archived\": true.")]
    async fn get_task(
        &self,
        Parameters(params): Parameters<TaskIdParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tool_ok(
            async {
                let tasks = store::load_all(&self.base).await?;
                match service::get_task(&tasks, &params.id) {
                    Ok(t) => {
                        let eff = service::effective_priorities(&tasks);
                        let ep = eff.get(&t.id);
                        ok_json(serde_json::to_value(t.detail(ep))?)
                    }
                    // Not in the active store — fall back to the archive (read-only).
                    Err(Error::TaskNotFound(id)) => {
                        match service::get_archived_task(&self.base, &params.id).await {
                            Ok(t) => {
                                let mut v = serde_json::to_value(t.detail(None))?;
                                if let Some(obj) = v.as_object_mut() {
                                    obj.insert("archived".into(), serde_json::Value::Bool(true));
                                }
                                ok_json(v)
                            }
                            // Neither active nor archived: report the original miss.
                            Err(_) => Err(Error::TaskNotFound(id)),
                        }
                    }
                    Err(e) => Err(e),
                }
            }
            .await,
        )
    }

    #[tool(description = "Create a new task")]
    async fn create_task(
        &self,
        Parameters(params): Parameters<CreateTaskParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tool_ok(
            async {
                let tasks = store::load_all(&self.base).await?;
                let priority = params.priority.unwrap_or(Priority::P2);
                let task_type = params.task_type.unwrap_or_default();

                let t = service::create_task(
                    &self.base,
                    &tasks,
                    params.title,
                    priority,
                    params.tags.unwrap_or_default(),
                    params.depends_on.unwrap_or_default(),
                    params.parent,
                    params.body.unwrap_or_default(),
                    task_type,
                )?;
                ok_json(serde_json::to_value(t.summary(None))?)
            }
            .await,
        )
    }

    #[tool(description = "Update task fields")]
    async fn update_task(
        &self,
        Parameters(params): Parameters<UpdateTaskParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tool_ok(
            async {
                let tasks = store::load_all(&self.base).await?;
                // Map MCP parent: None = unchanged, "" = clear, "id" = set
                let parent_update: Option<Option<String>> = params
                    .parent
                    .map(|p| if p.is_empty() { None } else { Some(p) });
                let t = service::update_task(
                    &self.base,
                    &tasks,
                    &params.id,
                    params.status,
                    params.priority,
                    params.tags,
                    params.assignee,
                    params.body,
                    params.title,
                    parent_update,
                )?;
                ok_json(serde_json::to_value(t.summary(None))?)
            }
            .await,
        )
    }

    #[tool(description = "Start a task (set status to in_progress)")]
    async fn start_task(
        &self,
        Parameters(params): Parameters<TaskIdParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tool_ok(
            async {
                let tasks = store::load_all(&self.base).await?;
                let t = service::set_status(&self.base, &tasks, &params.id, Status::InProgress)?;
                ok_json(serde_json::to_value(t.summary(None))?)
            }
            .await,
        )
    }

    #[tool(description = "Complete a task (set status to done)")]
    async fn complete_task(
        &self,
        Parameters(params): Parameters<TaskIdParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tool_ok(
            async {
                let tasks = store::load_all(&self.base).await?;
                let t = service::set_status(&self.base, &tasks, &params.id, Status::Done)?;
                ok_json(serde_json::to_value(t.summary(None))?)
            }
            .await,
        )
    }

    #[tool(description = "Add a dependency between tasks")]
    async fn add_dependency(
        &self,
        Parameters(params): Parameters<DepParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tool_ok(
            async {
                let tasks = store::load_all(&self.base).await?;
                let t =
                    service::add_dependency(&self.base, &tasks, &params.id, &params.depends_on)?;
                ok_json(serde_json::to_value(t.summary(None))?)
            }
            .await,
        )
    }

    #[tool(description = "Remove a dependency between tasks")]
    async fn remove_dependency(
        &self,
        Parameters(params): Parameters<DepParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tool_ok(
            async {
                let tasks = store::load_all(&self.base).await?;
                let t =
                    service::remove_dependency(&self.base, &tasks, &params.id, &params.depends_on)?;
                ok_json(serde_json::to_value(t.summary(None))?)
            }
            .await,
        )
    }

    #[tool(description = "Search tasks by text query")]
    async fn search_tasks(
        &self,
        Parameters(params): Parameters<SearchParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tool_ok(
            async {
                let tasks = store::load_all(&self.base).await?;
                let include_all = !params.active_only.unwrap_or(false);
                let mut results = service::search_tasks(&tasks, &params.query, include_all);
                if let Some(limit) = params.limit {
                    results.truncate(limit as usize);
                }
                let summaries: Vec<_> = results.iter().map(|t| t.summary(None)).collect();
                ok_json(serde_json::json!(summaries))
            }
            .await,
        )
    }

    #[tool(description = "Cancel a task (set status to cancelled)")]
    async fn cancel_task(
        &self,
        Parameters(params): Parameters<TaskIdParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tool_ok(
            async {
                let tasks = store::load_all(&self.base).await?;
                let t = service::set_status(&self.base, &tasks, &params.id, Status::Cancelled)?;
                ok_json(serde_json::to_value(t.summary(None))?)
            }
            .await,
        )
    }

    #[tool(
        description = "DEPRECATED: Permanently hard-deletes cancelled (and optionally done) tasks. \
        Prefer archive_task (no id → sweep) which moves settled tasks to the archive instead of \
        destroying them, keeping history recoverable via restore_task. \
        prune_tasks remains available for cases where permanent deletion is intentional."
    )]
    async fn prune_tasks(
        &self,
        Parameters(params): Parameters<PruneParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tool_ok(
            async {
                let tasks = store::load_all(&self.base).await?;
                let include_done = params.include_done.unwrap_or(false);
                let deleted = service::prune_tasks(&self.base, &tasks, include_done)?;
                let summaries: Vec<_> = deleted.iter().map(|t| t.summary(None)).collect();
                ok_json(serde_json::json!(summaries))
            }
            .await,
        )
    }

    #[tool(description = "Permanently delete a task by ID")]
    async fn delete_task(
        &self,
        Parameters(params): Parameters<TaskIdParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tool_ok(
            async {
                let tasks = store::load_all(&self.base).await?;
                let t = service::delete_task(&self.base, &tasks, &params.id)?;
                ok_json(serde_json::to_value(t.summary(None))?)
            }
            .await,
        )
    }

    #[tool(description = "Get the dependency graph as a bounded adjacency list. \
        Excludes isolated and done/cancelled nodes by default.")]
    async fn get_graph(
        &self,
        Parameters(params): Parameters<GetGraphParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tool_ok(
            async {
                let tasks = store::load_all(&self.base).await?;
                let graph = service::build_graph(&tasks);
                let include_done = params.include_done.unwrap_or(false);
                let limit = params.limit.map(|v| v as usize);
                let adj = graph.bounded_adjacency_list(
                    &tasks,
                    include_done,
                    params.epic.as_deref(),
                    limit,
                );
                ok_json(serde_json::json!(adj))
            }
            .await,
        )
    }

    #[tool(
        description = "Return the children of an epic in topological execution order (plan view)"
    )]
    async fn plan_epic(
        &self,
        Parameters(params): Parameters<PlanEpicParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tool_ok(
            async {
                let tasks = store::load_all(&self.base).await?;
                let plan = service::plan_epic(&tasks, &params.id)?;
                // Cyclic children cannot be ordered; append them so nothing is lost.
                let all: Vec<&Task> = plan
                    .tasks
                    .iter()
                    .chain(plan.cyclic.iter())
                    .copied()
                    .collect();
                let eff = service::effective_priorities(&tasks);
                let summaries: Vec<_> = all.iter().map(|t| t.summary(eff.get(&t.id))).collect();
                ok_json(serde_json::json!(summaries))
            }
            .await,
        )
    }

    #[tool(description = "Archive a task (and its settled epic children) by ID, \
        or sweep all archivable tasks when no ID is given. \
        Only Done/Cancelled tasks with no active dependents can be archived. \
        Archived tasks are hidden from all active-task tools.")]
    async fn archive_task(
        &self,
        Parameters(params): Parameters<ArchiveTaskParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tool_ok(
            async {
                let tasks = store::load_all(&self.base).await?;
                let archived_ids = match params.id {
                    Some(ref id) => service::archive_task(&self.base, &tasks, id)?,
                    None => service::archive_all(&self.base, &tasks)?,
                };
                ok_json(serde_json::json!(archived_ids))
            }
            .await,
        )
    }

    #[tool(
        description = "Restore an archived task (and its archived dependencies/parent epic) \
        back to the active store."
    )]
    async fn restore_task(
        &self,
        Parameters(params): Parameters<RestoreTaskParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tool_ok(
            async {
                let restored_ids = service::restore_task(&self.base, &params.id).await?;
                ok_json(serde_json::json!(restored_ids))
            }
            .await,
        )
    }

    #[tool(description = "List archived tasks sorted by most recently updated. \
        Use limit to cap the number returned.")]
    async fn list_archived(
        &self,
        Parameters(params): Parameters<ListArchivedParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        tool_ok(
            async {
                let limit = params.limit.map(|v| v as usize);
                let archived = service::list_archive(&self.base, limit).await?;
                let summaries: Vec<_> = archived.iter().map(|t| t.summary(None)).collect();
                ok_json(serde_json::json!(summaries))
            }
            .await,
        )
    }

    #[tool(description = "List all epics with progress summary")]
    async fn list_epics(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        tool_ok(
            async {
                let tasks = store::load_all(&self.base).await?;
                let mut epics: Vec<&task::Task> =
                    tasks.values().filter(|t| t.task_type.is_epic()).collect();
                epics.sort_by(|a, b| a.priority.cmp(&b.priority).then(a.created.cmp(&b.created)));
                let summaries: Vec<_> = epics
                    .iter()
                    .map(|t| t.epic_summary(service::epic_progress(&tasks, &t.id)))
                    .collect();
                ok_json(serde_json::json!(summaries))
            }
            .await,
        )
    }
}

#[tool_handler]
impl ServerHandler for BeaMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("bears", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "bears is a file-based task tracker. Use tools to manage tasks and dependencies. \
                Completed or cancelled tasks can be archived with archive_task to keep the active \
                list clean. Use list_archived to browse the archive and restore_task to bring a \
                task back to active."
                    .to_string(),
            )
    }
}

impl BeaMcp {
    pub(super) fn build_tool_router() -> ToolRouter<Self> {
        Self::tool_router()
    }
}

#[cfg(test)]
mod tests {
    use rmcp::handler::server::wrapper::Parameters;
    use rmcp::model::*;

    use crate::store;
    use crate::task::{Priority, Status, TaskType};

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

    #[tokio::test]
    async fn test_tool_create_and_list() {
        let (_tmp, mcp) = setup();
        let result = mcp
            .create_task(Parameters(CreateTaskParams {
                title: "Test task".into(),
                priority: Some(Priority::P1),
                tags: Some(vec!["backend".into()]),
                depends_on: None,
                parent: None,
                body: None,
                task_type: None,
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
                epic: None,
                limit: None,
                active_only: None,
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
                task_type: None,
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
    async fn test_tool_get_task_falls_back_to_archive() {
        let (_tmp, mcp) = setup();
        // Create, complete, and archive a task.
        let created = mcp
            .create_task(Parameters(CreateTaskParams {
                title: "Archived detail".into(),
                priority: None,
                tags: None,
                depends_on: None,
                parent: None,
                body: Some("archived body".into()),
                task_type: None,
            }))
            .await
            .unwrap();
        let id = extract_json(&created)["id"].as_str().unwrap().to_string();
        mcp.complete_task(Parameters(TaskIdParams { id: id.clone() }))
            .await
            .unwrap();
        mcp.archive_task(Parameters(ArchiveTaskParams {
            id: Some(id.clone()),
        }))
        .await
        .unwrap();

        // get_task on the archived id resolves via the archive fallback and is
        // flagged as archived.
        let detail = mcp
            .get_task(Parameters(TaskIdParams { id: id.clone() }))
            .await
            .unwrap();
        let json = extract_json(&detail);
        assert_eq!(json["title"], "Archived detail");
        assert_eq!(json["archived"], true);

        // A genuinely unknown id (neither active nor archived) reports an
        // in-band tool error.
        let missing = mcp
            .get_task(Parameters(TaskIdParams { id: "nope".into() }))
            .await
            .unwrap();
        assert_eq!(missing.is_error, Some(true));
        assert!(extract_text(&missing).contains("not found"));
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
                task_type: None,
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
                task_type: None,
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
            task_type: None,
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
            .create_task(Parameters(CreateTaskParams {
                title: "A".into(),
                priority: None,
                tags: None,
                depends_on: None,
                parent: None,
                body: None,
                task_type: None,
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
                task_type: None,
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
            task_type: None,
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
            task_type: None,
        }))
        .await
        .unwrap();

        let results = mcp
            .search_tasks(Parameters(SearchParams {
                query: "OAuth".into(),
                limit: None,
                active_only: None,
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
                task_type: None,
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
                task_type: None,
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
            task_type: None,
        }))
        .await
        .unwrap();

        let graph = mcp
            .get_graph(Parameters(GetGraphParams {
                include_done: None,
                epic: None,
                limit: None,
            }))
            .await
            .unwrap();
        let json = extract_json(&graph);
        assert!(json.is_object());
    }

    #[tokio::test]
    async fn test_tool_get_graph_bounded() {
        let (_tmp, mcp) = setup();

        // Create A -> B dependency (both active)
        let t_a = mcp
            .create_task(Parameters(CreateTaskParams {
                title: "A".into(),
                priority: None,
                tags: None,
                depends_on: None,
                parent: None,
                body: None,
                task_type: None,
            }))
            .await
            .unwrap();
        let id_a = extract_json(&t_a)["id"].as_str().unwrap().to_string();

        mcp.create_task(Parameters(CreateTaskParams {
            title: "B".into(),
            priority: None,
            tags: None,
            depends_on: Some(vec![id_a.clone()]),
            parent: None,
            body: None,
            task_type: None,
        }))
        .await
        .unwrap();

        // Create a done isolated task C (no deps, no dependents)
        let t_c = mcp
            .create_task(Parameters(CreateTaskParams {
                title: "C (isolated)".into(),
                priority: None,
                tags: None,
                depends_on: None,
                parent: None,
                body: None,
                task_type: None,
            }))
            .await
            .unwrap();
        let id_c = extract_json(&t_c)["id"].as_str().unwrap().to_string();
        mcp.complete_task(Parameters(TaskIdParams { id: id_c.clone() }))
            .await
            .unwrap();

        // Create an open isolated task D (no edges)
        let t_d = mcp
            .create_task(Parameters(CreateTaskParams {
                title: "D (isolated open)".into(),
                priority: None,
                tags: None,
                depends_on: None,
                parent: None,
                body: None,
                task_type: None,
            }))
            .await
            .unwrap();
        let id_d = extract_json(&t_d)["id"].as_str().unwrap().to_string();

        // Default get_graph: excludes done (C) and isolated (D)
        let graph = mcp
            .get_graph(Parameters(GetGraphParams {
                include_done: None,
                epic: None,
                limit: None,
            }))
            .await
            .unwrap();
        let json = extract_json(&graph);
        let obj = json.as_object().unwrap();
        // A and B should be in the graph (they have an edge between them)
        assert!(obj.contains_key(id_a.as_str()), "A should be in graph");
        // C is done → excluded
        assert!(
            !obj.contains_key(id_c.as_str()),
            "done task C should be excluded"
        );
        // D is isolated (no edges) → excluded
        assert!(
            !obj.contains_key(id_d.as_str()),
            "isolated task D should be excluded"
        );

        // include_done=true: C is now eligible, but C is still isolated → still excluded
        let graph_all = mcp
            .get_graph(Parameters(GetGraphParams {
                include_done: Some(true),
                epic: None,
                limit: None,
            }))
            .await
            .unwrap();
        let obj_all = extract_json(&graph_all);
        let obj_all = obj_all.as_object().unwrap();
        // C is done but still isolated → excluded
        assert!(
            !obj_all.contains_key(id_c.as_str()),
            "isolated done task C should still be excluded"
        );
    }

    #[tokio::test]
    async fn test_tool_update_title() {
        let (_tmp, mcp) = setup();
        let result = mcp
            .create_task(Parameters(CreateTaskParams {
                title: "Original Title".into(),
                priority: None,
                tags: None,
                depends_on: None,
                parent: None,
                body: None,
                task_type: None,
            }))
            .await
            .unwrap();
        let id = extract_json(&result)["id"].as_str().unwrap().to_string();

        // Rename the task via update_task
        let updated = mcp
            .update_task(Parameters(UpdateTaskParams {
                id: id.clone(),
                title: Some("Renamed Title".into()),
                status: None,
                priority: None,
                tags: None,
                assignee: None,
                body: None,
                parent: None,
            }))
            .await
            .unwrap();
        let json = extract_json(&updated);
        assert_eq!(json["title"], "Renamed Title");
        assert_eq!(json["id"], id);

        // Confirm the change persists when fetched
        let detail = mcp.get_task(Parameters(TaskIdParams { id })).await.unwrap();
        assert_eq!(extract_json(&detail)["title"], "Renamed Title");
    }

    #[tokio::test]
    async fn test_tool_list_limit_and_active_only() {
        let (_tmp, mcp) = setup();

        // Create 3 tasks; complete one
        for title in &["Alpha", "Beta", "Gamma"] {
            mcp.create_task(Parameters(CreateTaskParams {
                title: (*title).into(),
                priority: None,
                tags: None,
                depends_on: None,
                parent: None,
                body: None,
                task_type: None,
            }))
            .await
            .unwrap();
        }

        // Complete "Alpha"
        let all = mcp
            .list_all_tasks(Parameters(ListTasksFilterParams {
                status: None,
                priority: None,
                tag: None,
                epic: None,
                limit: None,
                active_only: None,
            }))
            .await
            .unwrap();
        let arr = extract_json(&all);
        let alpha_id = arr
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["title"] == "Alpha")
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        mcp.complete_task(Parameters(TaskIdParams {
            id: alpha_id.clone(),
        }))
        .await
        .unwrap();

        // active_only=true should exclude the done task
        let active = mcp
            .list_all_tasks(Parameters(ListTasksFilterParams {
                status: None,
                priority: None,
                tag: None,
                epic: None,
                limit: None,
                active_only: Some(true),
            }))
            .await
            .unwrap();
        let active_arr = extract_json(&active);
        let active_arr = active_arr.as_array().unwrap();
        assert_eq!(active_arr.len(), 2, "done task excluded with active_only");
        assert!(
            active_arr.iter().all(|x| x["id"] != alpha_id),
            "completed task should not appear"
        );

        // active_only=false (default) shows all 3
        let all2 = mcp
            .list_all_tasks(Parameters(ListTasksFilterParams {
                status: None,
                priority: None,
                tag: None,
                epic: None,
                limit: None,
                active_only: Some(false),
            }))
            .await
            .unwrap();
        assert_eq!(extract_json(&all2).as_array().unwrap().len(), 3);

        // limit=1 returns only one
        let limited = mcp
            .list_all_tasks(Parameters(ListTasksFilterParams {
                status: None,
                priority: None,
                tag: None,
                epic: None,
                limit: Some(1),
                active_only: None,
            }))
            .await
            .unwrap();
        assert_eq!(extract_json(&limited).as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_tool_search_limit_and_active_only() {
        let (_tmp, mcp) = setup();

        // Create 3 tasks all matching query "task"
        for title in &["task one", "task two", "task three"] {
            mcp.create_task(Parameters(CreateTaskParams {
                title: (*title).into(),
                priority: None,
                tags: None,
                depends_on: None,
                parent: None,
                body: None,
                task_type: None,
            }))
            .await
            .unwrap();
        }

        // Get id of "task one" and complete it
        let all = mcp
            .search_tasks(Parameters(SearchParams {
                query: "task one".into(),
                limit: None,
                active_only: None,
            }))
            .await
            .unwrap();
        let one_id = extract_json(&all).as_array().unwrap()[0]["id"]
            .as_str()
            .unwrap()
            .to_string();
        mcp.complete_task(Parameters(TaskIdParams { id: one_id.clone() }))
            .await
            .unwrap();

        // active_only=true excludes done
        let active = mcp
            .search_tasks(Parameters(SearchParams {
                query: "task".into(),
                limit: None,
                active_only: Some(true),
            }))
            .await
            .unwrap();
        let active_arr = extract_json(&active);
        let active_arr = active_arr.as_array().unwrap();
        assert_eq!(active_arr.len(), 2, "done task excluded with active_only");

        // limit=1 caps results
        let limited = mcp
            .search_tasks(Parameters(SearchParams {
                query: "task".into(),
                limit: Some(1),
                active_only: None,
            }))
            .await
            .unwrap();
        assert_eq!(extract_json(&limited).as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_tool_plan_epic() {
        let (_tmp, mcp) = setup();

        // Create an epic
        let epic = mcp
            .create_task(Parameters(CreateTaskParams {
                title: "My Epic".into(),
                priority: None,
                tags: None,
                depends_on: None,
                parent: None,
                body: None,
                task_type: Some(TaskType::Epic),
            }))
            .await
            .unwrap();
        let epic_id = extract_json(&epic)["id"].as_str().unwrap().to_string();

        // Create a linear chain: c1 <- c2 <- c3
        let c1 = mcp
            .create_task(Parameters(CreateTaskParams {
                title: "Step 1".into(),
                priority: None,
                tags: None,
                depends_on: None,
                parent: Some(epic_id.clone()),
                body: None,
                task_type: None,
            }))
            .await
            .unwrap();
        let id_c1 = extract_json(&c1)["id"].as_str().unwrap().to_string();

        let c2 = mcp
            .create_task(Parameters(CreateTaskParams {
                title: "Step 2".into(),
                priority: None,
                tags: None,
                depends_on: Some(vec![id_c1.clone()]),
                parent: Some(epic_id.clone()),
                body: None,
                task_type: None,
            }))
            .await
            .unwrap();
        let id_c2 = extract_json(&c2)["id"].as_str().unwrap().to_string();

        // Create an independent sibling
        mcp.create_task(Parameters(CreateTaskParams {
            title: "Independent Step".into(),
            priority: None,
            tags: None,
            depends_on: None,
            parent: Some(epic_id.clone()),
            body: None,
            task_type: None,
        }))
        .await
        .unwrap();

        let result = mcp
            .plan_epic(Parameters(PlanEpicParams {
                id: epic_id.clone(),
            }))
            .await
            .unwrap();
        assert!(!result.is_error.unwrap_or(false));
        let json = extract_json(&result);
        let arr = json.as_array().unwrap();
        // All 3 children returned
        assert_eq!(arr.len(), 3);
        // c1 must appear before c2 (dependency order)
        let pos_c1 = arr
            .iter()
            .position(|x| x["id"] == id_c1)
            .expect("c1 in plan");
        let pos_c2 = arr
            .iter()
            .position(|x| x["id"] == id_c2)
            .expect("c2 in plan");
        assert!(pos_c1 < pos_c2, "c1 must precede c2 in execution order");

        // Calling plan_epic on a non-epic task returns an error
        let non_epic_result = mcp
            .plan_epic(Parameters(PlanEpicParams { id: id_c1 }))
            .await
            .unwrap();
        assert_eq!(non_epic_result.is_error, Some(true));
    }

    #[tokio::test]
    async fn test_tool_reparent_set_and_clear() {
        let (_tmp, mcp) = setup();

        // Create an epic
        let epic = mcp
            .create_task(Parameters(CreateTaskParams {
                title: "My Epic".into(),
                priority: None,
                tags: None,
                depends_on: None,
                parent: None,
                body: None,
                task_type: Some(TaskType::Epic),
            }))
            .await
            .unwrap();
        let epic_id = extract_json(&epic)["id"].as_str().unwrap().to_string();

        // Create a task without a parent
        let task = mcp
            .create_task(Parameters(CreateTaskParams {
                title: "Child Task".into(),
                priority: None,
                tags: None,
                depends_on: None,
                parent: None,
                body: None,
                task_type: None,
            }))
            .await
            .unwrap();
        let task_id = extract_json(&task)["id"].as_str().unwrap().to_string();

        // Set parent to the epic
        let updated = mcp
            .update_task(Parameters(UpdateTaskParams {
                id: task_id.clone(),
                title: None,
                status: None,
                priority: None,
                tags: None,
                assignee: None,
                body: None,
                parent: Some(epic_id.clone()),
            }))
            .await
            .unwrap();
        let json = extract_json(&updated);
        assert_eq!(json["id"], task_id);

        // Confirm the parent is set via get_task
        let detail = mcp
            .get_task(Parameters(TaskIdParams {
                id: task_id.clone(),
            }))
            .await
            .unwrap();
        assert_eq!(extract_json(&detail)["parent"], epic_id);

        // Clear parent with empty string
        let cleared = mcp
            .update_task(Parameters(UpdateTaskParams {
                id: task_id.clone(),
                title: None,
                status: None,
                priority: None,
                tags: None,
                assignee: None,
                body: None,
                parent: Some("".into()),
            }))
            .await
            .unwrap();
        assert!(!cleared.is_error.unwrap_or(false));

        // Confirm parent is cleared
        let detail2 = mcp
            .get_task(Parameters(TaskIdParams {
                id: task_id.clone(),
            }))
            .await
            .unwrap();
        assert!(
            extract_json(&detail2)["parent"].is_null(),
            "parent should be null after clearing"
        );
    }

    #[tokio::test]
    async fn test_tool_reparent_invalid_parent() {
        let (_tmp, mcp) = setup();
        let task = mcp
            .create_task(Parameters(CreateTaskParams {
                title: "Task".into(),
                priority: None,
                tags: None,
                depends_on: None,
                parent: None,
                body: None,
                task_type: None,
            }))
            .await
            .unwrap();
        let task_id = extract_json(&task)["id"].as_str().unwrap().to_string();

        // Attempt to set a non-existent parent
        let result = mcp
            .update_task(Parameters(UpdateTaskParams {
                id: task_id,
                title: None,
                status: None,
                priority: None,
                tags: None,
                assignee: None,
                body: None,
                parent: Some("nonexistent".into()),
            }))
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(true));
    }

    // ─── Archive tool tests ───────────────────────────────────────────────────

    #[tokio::test]
    async fn test_tool_archive_hides_from_list_and_ready() {
        let (_tmp, mcp) = setup();

        // Create two tasks: t1 (no deps), t2 depends on t1
        let t1 = mcp
            .create_task(Parameters(CreateTaskParams {
                title: "Base task".into(),
                priority: None,
                tags: None,
                depends_on: None,
                parent: None,
                body: None,
                task_type: None,
            }))
            .await
            .unwrap();
        let id1 = extract_json(&t1)["id"].as_str().unwrap().to_string();

        mcp.create_task(Parameters(CreateTaskParams {
            title: "Dependent task".into(),
            priority: None,
            tags: None,
            depends_on: Some(vec![id1.clone()]),
            parent: None,
            body: None,
            task_type: None,
        }))
        .await
        .unwrap();

        // Complete t1 so it's archivable (no active tasks depend on a done task... wait,
        // t2 depends on t1 and t2 is open — t1 is NOT archivable yet)
        mcp.complete_task(Parameters(TaskIdParams { id: id1.clone() }))
            .await
            .unwrap();

        // t1 is done but t2 (open) depends on it — archive should fail
        let fail = mcp
            .archive_task(Parameters(ArchiveTaskParams {
                id: Some(id1.clone()),
            }))
            .await
            .unwrap();
        assert_eq!(fail.is_error, Some(true));

        // Complete t2 as well — now t1 has no active dependents
        let all = mcp
            .list_all_tasks(Parameters(ListTasksFilterParams {
                status: None,
                priority: None,
                tag: None,
                epic: None,
                limit: None,
                active_only: None,
            }))
            .await
            .unwrap();
        let arr = extract_json(&all);
        let id2 = arr
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["title"] == "Dependent task")
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();

        mcp.complete_task(Parameters(TaskIdParams { id: id2.clone() }))
            .await
            .unwrap();

        // Now archive t1 — t2 is done so t1 has no active dependents
        let archived = mcp
            .archive_task(Parameters(ArchiveTaskParams {
                id: Some(id1.clone()),
            }))
            .await
            .unwrap();
        assert!(!archived.is_error.unwrap_or(false));
        let archived_ids = extract_json(&archived);
        assert!(archived_ids.as_array().unwrap().iter().any(|v| v == &id1));

        // t1 should no longer appear in list_all_tasks
        let all2 = mcp
            .list_all_tasks(Parameters(ListTasksFilterParams {
                status: None,
                priority: None,
                tag: None,
                epic: None,
                limit: None,
                active_only: None,
            }))
            .await
            .unwrap();
        let arr2 = extract_json(&all2);
        assert!(
            arr2.as_array().unwrap().iter().all(|x| x["id"] != id1),
            "archived task must not appear in list_all_tasks"
        );

        // t1 should appear in list_archived
        let listed = mcp
            .list_archived(Parameters(ListArchivedParams { limit: None }))
            .await
            .unwrap();
        let arr3 = extract_json(&listed);
        assert!(
            arr3.as_array().unwrap().iter().any(|x| x["id"] == id1),
            "archived task must appear in list_archived"
        );
    }

    #[tokio::test]
    async fn test_tool_archive_sweep_no_id() {
        let (_tmp, mcp) = setup();

        // Create two independent tasks, both done
        let t1 = mcp
            .create_task(Parameters(CreateTaskParams {
                title: "Done 1".into(),
                priority: None,
                tags: None,
                depends_on: None,
                parent: None,
                body: None,
                task_type: None,
            }))
            .await
            .unwrap();
        let id1 = extract_json(&t1)["id"].as_str().unwrap().to_string();

        let t2 = mcp
            .create_task(Parameters(CreateTaskParams {
                title: "Done 2".into(),
                priority: None,
                tags: None,
                depends_on: None,
                parent: None,
                body: None,
                task_type: None,
            }))
            .await
            .unwrap();
        let id2 = extract_json(&t2)["id"].as_str().unwrap().to_string();

        mcp.complete_task(Parameters(TaskIdParams { id: id1.clone() }))
            .await
            .unwrap();
        mcp.complete_task(Parameters(TaskIdParams { id: id2.clone() }))
            .await
            .unwrap();

        // Sweep: no id → archive all archivable tasks
        let result = mcp
            .archive_task(Parameters(ArchiveTaskParams { id: None }))
            .await
            .unwrap();
        assert!(!result.is_error.unwrap_or(false));
        let ids = extract_json(&result);
        let ids_arr = ids.as_array().unwrap();
        assert!(ids_arr.len() >= 2, "both done tasks should be archived");
        assert!(ids_arr.iter().any(|v| v == &id1));
        assert!(ids_arr.iter().any(|v| v == &id2));

        // Active list should be empty
        let all = mcp
            .list_all_tasks(Parameters(ListTasksFilterParams {
                status: None,
                priority: None,
                tag: None,
                epic: None,
                limit: None,
                active_only: None,
            }))
            .await
            .unwrap();
        assert_eq!(extract_json(&all).as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_tool_restore_task_brings_back_to_active() {
        let (_tmp, mcp) = setup();

        // Create a task, complete it, archive it
        let t = mcp
            .create_task(Parameters(CreateTaskParams {
                title: "Will be archived".into(),
                priority: None,
                tags: None,
                depends_on: None,
                parent: None,
                body: None,
                task_type: None,
            }))
            .await
            .unwrap();
        let id = extract_json(&t)["id"].as_str().unwrap().to_string();

        mcp.complete_task(Parameters(TaskIdParams { id: id.clone() }))
            .await
            .unwrap();
        mcp.archive_task(Parameters(ArchiveTaskParams {
            id: Some(id.clone()),
        }))
        .await
        .unwrap();

        // Verify it's archived and not active
        let archived_before = mcp
            .list_archived(Parameters(ListArchivedParams { limit: None }))
            .await
            .unwrap();
        let arr = extract_json(&archived_before);
        assert!(arr.as_array().unwrap().iter().any(|x| x["id"] == id));

        let active_before = mcp
            .list_all_tasks(Parameters(ListTasksFilterParams {
                status: None,
                priority: None,
                tag: None,
                epic: None,
                limit: None,
                active_only: None,
            }))
            .await
            .unwrap();
        assert!(
            extract_json(&active_before)
                .as_array()
                .unwrap()
                .iter()
                .all(|x| x["id"] != id)
        );

        // Restore
        let restored = mcp
            .restore_task(Parameters(RestoreTaskParams { id: id.clone() }))
            .await
            .unwrap();
        assert!(!restored.is_error.unwrap_or(false));
        let restored_ids = extract_json(&restored);
        assert!(restored_ids.as_array().unwrap().iter().any(|v| v == &id));

        // Now it should be active again and list_ready should see it (status=done, won't be ready,
        // but it IS in the active list)
        let active_after = mcp
            .list_all_tasks(Parameters(ListTasksFilterParams {
                status: None,
                priority: None,
                tag: None,
                epic: None,
                limit: None,
                active_only: None,
            }))
            .await
            .unwrap();
        assert!(
            extract_json(&active_after)
                .as_array()
                .unwrap()
                .iter()
                .any(|x| x["id"] == id),
            "restored task must appear in active list"
        );

        // And gone from archive
        let archived_after = mcp
            .list_archived(Parameters(ListArchivedParams { limit: None }))
            .await
            .unwrap();
        assert!(
            extract_json(&archived_after)
                .as_array()
                .unwrap()
                .iter()
                .all(|x| x["id"] != id)
        );
    }

    #[tokio::test]
    async fn test_tool_restore_then_ready() {
        // Archive a done task that had no deps, restore it (status=done → won't be ready),
        // then open it to verify it shows in list_ready.
        let (_tmp, mcp) = setup();

        let t = mcp
            .create_task(Parameters(CreateTaskParams {
                title: "Restore me".into(),
                priority: None,
                tags: None,
                depends_on: None,
                parent: None,
                body: None,
                task_type: None,
            }))
            .await
            .unwrap();
        let id = extract_json(&t)["id"].as_str().unwrap().to_string();

        mcp.complete_task(Parameters(TaskIdParams { id: id.clone() }))
            .await
            .unwrap();
        mcp.archive_task(Parameters(ArchiveTaskParams {
            id: Some(id.clone()),
        }))
        .await
        .unwrap();

        // Restore the task
        mcp.restore_task(Parameters(RestoreTaskParams { id: id.clone() }))
            .await
            .unwrap();

        // Re-open the task so it becomes ready
        mcp.update_task(Parameters(UpdateTaskParams {
            id: id.clone(),
            title: None,
            status: Some(Status::Open),
            priority: None,
            tags: None,
            assignee: None,
            body: None,
            parent: None,
        }))
        .await
        .unwrap();

        // Now it should appear in list_ready
        let ready = mcp
            .list_ready(Parameters(ListReadyParams {
                limit: None,
                tag: None,
                epic: None,
            }))
            .await
            .unwrap();
        let arr = extract_json(&ready);
        assert!(
            arr.as_array().unwrap().iter().any(|x| x["id"] == id),
            "restored and reopened task must appear in list_ready"
        );
    }

    #[tokio::test]
    async fn test_tool_list_archived_with_limit() {
        let (_tmp, mcp) = setup();

        for title in &["A", "B", "C"] {
            let t = mcp
                .create_task(Parameters(CreateTaskParams {
                    title: (*title).into(),
                    priority: None,
                    tags: None,
                    depends_on: None,
                    parent: None,
                    body: None,
                    task_type: None,
                }))
                .await
                .unwrap();
            let id = extract_json(&t)["id"].as_str().unwrap().to_string();
            mcp.complete_task(Parameters(TaskIdParams { id: id.clone() }))
                .await
                .unwrap();
            mcp.archive_task(Parameters(ArchiveTaskParams { id: Some(id) }))
                .await
                .unwrap();
        }

        let all = mcp
            .list_archived(Parameters(ListArchivedParams { limit: None }))
            .await
            .unwrap();
        assert_eq!(extract_json(&all).as_array().unwrap().len(), 3);

        let limited = mcp
            .list_archived(Parameters(ListArchivedParams { limit: Some(2) }))
            .await
            .unwrap();
        assert_eq!(extract_json(&limited).as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_tool_restore_nonexistent_archived_errors() {
        let (_tmp, mcp) = setup();

        let result = mcp
            .restore_task(Parameters(RestoreTaskParams {
                id: "nonexistent".into(),
            }))
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(true));
    }

    // ─── xja: end-to-end archive visibility and integrity (MCP layer) ─────────

    /// Archived task is hidden from search_tasks.
    #[tokio::test]
    async fn test_tool_archived_hidden_from_search() {
        let (_tmp, mcp) = setup();

        let t = mcp
            .create_task(Parameters(CreateTaskParams {
                title: "Searchable archived task".into(),
                priority: None,
                tags: None,
                depends_on: None,
                parent: None,
                body: None,
                task_type: None,
            }))
            .await
            .unwrap();
        let id = extract_json(&t)["id"].as_str().unwrap().to_string();

        mcp.complete_task(Parameters(TaskIdParams { id: id.clone() }))
            .await
            .unwrap();
        mcp.archive_task(Parameters(ArchiveTaskParams {
            id: Some(id.clone()),
        }))
        .await
        .unwrap();

        // search_tasks (default: includes done) must not return archived task
        let results = mcp
            .search_tasks(Parameters(SearchParams {
                query: "Searchable archived task".into(),
                limit: None,
                active_only: None,
            }))
            .await
            .unwrap();
        let arr = extract_json(&results);
        assert!(
            arr.as_array().unwrap().iter().all(|x| x["id"] != id),
            "archived task must not appear in search_tasks"
        );
    }

    /// Archived task is hidden from get_graph (even with include_done=true).
    #[tokio::test]
    async fn test_tool_archived_hidden_from_graph() {
        let (_tmp, mcp) = setup();

        // Create A → B dep chain; both done and archived via sweep
        let t_a = mcp
            .create_task(Parameters(CreateTaskParams {
                title: "Graph base".into(),
                priority: None,
                tags: None,
                depends_on: None,
                parent: None,
                body: None,
                task_type: None,
            }))
            .await
            .unwrap();
        let id_a = extract_json(&t_a)["id"].as_str().unwrap().to_string();

        let t_b = mcp
            .create_task(Parameters(CreateTaskParams {
                title: "Graph dependent".into(),
                priority: None,
                tags: None,
                depends_on: Some(vec![id_a.clone()]),
                parent: None,
                body: None,
                task_type: None,
            }))
            .await
            .unwrap();
        let id_b = extract_json(&t_b)["id"].as_str().unwrap().to_string();

        mcp.complete_task(Parameters(TaskIdParams { id: id_a.clone() }))
            .await
            .unwrap();
        mcp.complete_task(Parameters(TaskIdParams { id: id_b.clone() }))
            .await
            .unwrap();
        // Sweep archive
        mcp.archive_task(Parameters(ArchiveTaskParams { id: None }))
            .await
            .unwrap();

        // get_graph with include_done=true must not return archived nodes
        let graph = mcp
            .get_graph(Parameters(GetGraphParams {
                include_done: Some(true),
                epic: None,
                limit: None,
            }))
            .await
            .unwrap();
        let obj = extract_json(&graph);
        let obj = obj.as_object().unwrap();
        assert!(
            !obj.contains_key(id_a.as_str()),
            "archived node A must not appear in graph"
        );
        assert!(
            !obj.contains_key(id_b.as_str()),
            "archived node B must not appear in graph"
        );
    }

    /// Archived epic is hidden from list_epics.
    #[tokio::test]
    async fn test_tool_archived_epic_hidden_from_list_epics() {
        let (_tmp, mcp) = setup();

        let epic = mcp
            .create_task(Parameters(CreateTaskParams {
                title: "Hidden epic".into(),
                priority: None,
                tags: None,
                depends_on: None,
                parent: None,
                body: None,
                task_type: Some(TaskType::Epic),
            }))
            .await
            .unwrap();
        let epic_id = extract_json(&epic)["id"].as_str().unwrap().to_string();

        let child = mcp
            .create_task(Parameters(CreateTaskParams {
                title: "Only child".into(),
                priority: None,
                tags: None,
                depends_on: None,
                parent: Some(epic_id.clone()),
                body: None,
                task_type: None,
            }))
            .await
            .unwrap();
        let child_id = extract_json(&child)["id"].as_str().unwrap().to_string();

        // Complete child (epic auto-closes) then archive the epic
        mcp.complete_task(Parameters(TaskIdParams {
            id: child_id.clone(),
        }))
        .await
        .unwrap();
        mcp.archive_task(Parameters(ArchiveTaskParams {
            id: Some(epic_id.clone()),
        }))
        .await
        .unwrap();

        let epics = mcp.list_epics().await.unwrap();
        let arr = extract_json(&epics);
        assert!(
            arr.as_array().unwrap().iter().all(|x| x["id"] != epic_id),
            "archived epic must not appear in list_epics"
        );
    }

    /// dep add onto an archived task ID is rejected (treated as unknown).
    #[tokio::test]
    async fn test_tool_dep_add_onto_archived_id_is_rejected() {
        let (_tmp, mcp) = setup();

        let archived = mcp
            .create_task(Parameters(CreateTaskParams {
                title: "To archive".into(),
                priority: None,
                tags: None,
                depends_on: None,
                parent: None,
                body: None,
                task_type: None,
            }))
            .await
            .unwrap();
        let archived_id = extract_json(&archived)["id"].as_str().unwrap().to_string();

        mcp.complete_task(Parameters(TaskIdParams {
            id: archived_id.clone(),
        }))
        .await
        .unwrap();
        mcp.archive_task(Parameters(ArchiveTaskParams {
            id: Some(archived_id.clone()),
        }))
        .await
        .unwrap();

        let active = mcp
            .create_task(Parameters(CreateTaskParams {
                title: "Active task".into(),
                priority: None,
                tags: None,
                depends_on: None,
                parent: None,
                body: None,
                task_type: None,
            }))
            .await
            .unwrap();
        let active_id = extract_json(&active)["id"].as_str().unwrap().to_string();

        let result = mcp
            .add_dependency(Parameters(DepParams {
                id: active_id,
                depends_on: archived_id,
            }))
            .await
            .unwrap();
        assert_eq!(
            result.is_error,
            Some(true),
            "dep add onto archived id must return an error"
        );
    }

    /// prune_tasks hard-deletes from active store only — the archive is untouched.
    #[tokio::test]
    async fn test_tool_prune_never_touches_archive() {
        let (_tmp, mcp) = setup();

        let t_arch = mcp
            .create_task(Parameters(CreateTaskParams {
                title: "Archived task".into(),
                priority: None,
                tags: None,
                depends_on: None,
                parent: None,
                body: None,
                task_type: None,
            }))
            .await
            .unwrap();
        let arch_id = extract_json(&t_arch)["id"].as_str().unwrap().to_string();
        mcp.complete_task(Parameters(TaskIdParams {
            id: arch_id.clone(),
        }))
        .await
        .unwrap();
        mcp.archive_task(Parameters(ArchiveTaskParams {
            id: Some(arch_id.clone()),
        }))
        .await
        .unwrap();

        // Create a cancelled task in the active store for prune to consume
        let t_cancel = mcp
            .create_task(Parameters(CreateTaskParams {
                title: "Cancelled active".into(),
                priority: None,
                tags: None,
                depends_on: None,
                parent: None,
                body: None,
                task_type: None,
            }))
            .await
            .unwrap();
        let cancel_id = extract_json(&t_cancel)["id"].as_str().unwrap().to_string();
        mcp.cancel_task(Parameters(TaskIdParams {
            id: cancel_id.clone(),
        }))
        .await
        .unwrap();

        let pruned = mcp
            .prune_tasks(Parameters(PruneParams {
                include_done: Some(true),
            }))
            .await
            .unwrap();
        assert!(!pruned.is_error.unwrap_or(false));

        // Archived task must still be in list_archived
        let archived = mcp
            .list_archived(Parameters(ListArchivedParams { limit: None }))
            .await
            .unwrap();
        assert!(
            extract_json(&archived)
                .as_array()
                .unwrap()
                .iter()
                .any(|x| x["id"] == arch_id),
            "archived task must not be removed by prune"
        );
    }

    /// New task IDs are never reused from archived IDs.
    #[tokio::test]
    async fn test_tool_new_task_ids_do_not_reuse_archived() {
        let (_tmp, mcp) = setup();

        let t = mcp
            .create_task(Parameters(CreateTaskParams {
                title: "ID Guard".into(),
                priority: None,
                tags: None,
                depends_on: None,
                parent: None,
                body: None,
                task_type: None,
            }))
            .await
            .unwrap();
        let archived_id = extract_json(&t)["id"].as_str().unwrap().to_string();

        mcp.complete_task(Parameters(TaskIdParams {
            id: archived_id.clone(),
        }))
        .await
        .unwrap();
        mcp.archive_task(Parameters(ArchiveTaskParams {
            id: Some(archived_id.clone()),
        }))
        .await
        .unwrap();

        let mut new_ids = Vec::new();
        for i in 0..10 {
            let nt = mcp
                .create_task(Parameters(CreateTaskParams {
                    title: format!("New {i}"),
                    priority: None,
                    tags: None,
                    depends_on: None,
                    parent: None,
                    body: None,
                    task_type: None,
                }))
                .await
                .unwrap();
            new_ids.push(extract_json(&nt)["id"].as_str().unwrap().to_string());
        }

        assert!(
            !new_ids.contains(&archived_id),
            "archived ID {archived_id} must not be reused; new IDs: {new_ids:?}"
        );
    }
}
