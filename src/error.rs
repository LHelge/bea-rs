use std::path::PathBuf;

use crate::task::Status;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not initialized: no `.bears` directory found")]
    NotInitialized,

    #[error("task not found: {0}")]
    TaskNotFound(String),

    #[error("ambiguous prefix '{prefix}' matches multiple tasks: {matches}")]
    AmbiguousPrefix { prefix: String, matches: String },

    #[error("task {0} is not an epic — plans can only be generated for epics")]
    NotAnEpic(String),

    #[error("parent {0} is not an epic — only epics can have child tasks")]
    ParentNotEpic(String),

    #[error("adding dependency would create a cycle: {from} -> {to}")]
    CycleDetected { from: String, to: String },

    #[error("unknown dependency ID(s): {}", ids.join(", "))]
    UnknownDependency { ids: Vec<String> },

    #[error("invalid config: {reason}")]
    InvalidConfig { reason: String },

    #[error("{0}")]
    InvalidUsage(String),

    #[error("invalid frontmatter in {path}: {reason}")]
    InvalidFrontmatter { path: PathBuf, reason: String },

    #[error("editor failed: {reason}")]
    EditorFailed { reason: String },

    /// The task cannot be archived because active tasks depend on it.
    #[error("task {id} is not archivable — active dependents: {}", blockers.join(", "))]
    NotArchivable { id: String, blockers: Vec<String> },

    /// A workflow shortcut was used on a task whose status does not allow it.
    ///
    /// The workflow verbs (`release`, `review`, `reject`, `propose`, `accept`)
    /// each move a task along one specific edge; setting a status directly is
    /// always available as the escape hatch.
    #[error("cannot {action} task {id}: expected status {expected}, but it is {actual}")]
    InvalidStatus {
        id: String,
        action: &'static str,
        expected: Status,
        actual: Status,
    },

    /// A claim was refused because the task already belongs to someone else.
    ///
    /// Returned by the claiming primitives (`claim_task`, `assign_task`) after
    /// a fresh read from disk, so two workers racing for the same task cannot
    /// both walk away believing they hold it.
    #[error("task {id} is already claimed by '{assignee}'")]
    AlreadyClaimed { id: String, assignee: String },

    /// A fenced mutation was refused because the task's assignee no longer
    /// matches the token the caller claimed it with.
    ///
    /// This is the stale-writer guard: a worker whose task has been released,
    /// reassigned, or reaped gets this error instead of silently clobbering
    /// the new holder's state.
    #[error("fence violation on task {id}: expected assignee '{expected}', but it is '{actual}'")]
    FenceViolation {
        id: String,
        expected: String,
        actual: String,
    },

    /// The task is not found in the archive.
    #[error("task not found in archive: {0}")]
    NotArchived(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Yaml(#[from] serde_yml::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
