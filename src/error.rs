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
