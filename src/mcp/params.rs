use rmcp::schemars;
use serde::Deserialize;

use crate::task::{Priority, Status, TaskType};

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListReadyParams {
    /// Max number of results
    pub limit: Option<u64>,
    /// Filter by tag
    pub tag: Option<String>,
    /// Filter by parent epic ID
    pub epic: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListTasksFilterParams {
    /// Filter by status
    pub status: Option<Status>,
    /// Filter by priority
    pub priority: Option<Priority>,
    /// Filter by tag
    pub tag: Option<String>,
    /// Filter by parent epic ID
    pub epic: Option<String>,
    /// Max number of results
    pub limit: Option<u64>,
    /// Exclude done and cancelled tasks (default: false — show all)
    pub active_only: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TaskIdParams {
    /// Task ID
    pub id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateTaskParams {
    /// Task title
    pub title: String,
    /// Priority (default P2)
    pub priority: Option<Priority>,
    /// Tags
    pub tags: Option<Vec<String>>,
    /// IDs of tasks this depends on
    pub depends_on: Option<Vec<String>>,
    /// Parent task ID
    pub parent: Option<String>,
    /// Task body (markdown)
    pub body: Option<String>,
    /// Type: "task" (default) or "epic"
    #[serde(rename = "type")]
    pub task_type: Option<TaskType>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateTaskParams {
    /// Task ID
    pub id: String,
    /// New title
    pub title: Option<String>,
    /// New status
    pub status: Option<Status>,
    /// New priority
    pub priority: Option<Priority>,
    /// New tags (replaces existing)
    pub tags: Option<Vec<String>>,
    /// New assignee
    pub assignee: Option<String>,
    /// New body (markdown)
    pub body: Option<String>,
    /// Set parent epic ID (use empty string "" to clear parent)
    pub parent: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DepParams {
    /// Task that will depend on another
    pub id: String,
    /// Task to depend on
    pub depends_on: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchParams {
    /// Search query
    pub query: String,
    /// Max number of results
    pub limit: Option<u64>,
    /// Exclude done and cancelled tasks (default: false — show all)
    pub active_only: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PruneParams {
    /// Also permanently delete done tasks (default: only cancelled).
    /// Prefer archive_task (no id) over prune_tasks to preserve history.
    pub include_done: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PlanEpicParams {
    /// Epic task ID
    pub id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetGraphParams {
    /// Include done and cancelled tasks (default: false)
    pub include_done: Option<bool>,
    /// Filter to direct children of a given epic ID
    pub epic: Option<String>,
    /// Max number of nodes to return
    pub limit: Option<u64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ArchiveTaskParams {
    /// Task ID or prefix to archive. Omit to sweep all archivable tasks.
    pub id: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RestoreTaskParams {
    /// Archived task ID or prefix to restore
    pub id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListArchivedParams {
    /// Max number of archived tasks to return (most recently updated first)
    pub limit: Option<u64>,
}
