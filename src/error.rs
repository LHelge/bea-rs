use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not initialized — run `bea init` first")]
    NotInitialized,

    #[error("task not found: {0}")]
    TaskNotFound(String),

    #[error("adding dependency would create a cycle: {from} -> {to}")]
    CycleDetected { from: String, to: String },

    #[error("unknown dependency ID(s): {}", ids.join(", "))]
    UnknownDependency { ids: Vec<String> },

    #[error("invalid {field}: {value} (expected {expected})")]
    InvalidFilter {
        field: String,
        value: String,
        expected: String,
    },

    #[error("invalid config: {reason}")]
    InvalidConfig { reason: String },

    #[error("invalid frontmatter in {path}: {reason}")]
    InvalidFrontmatter { path: PathBuf, reason: String },

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Yaml(#[from] serde_yaml::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
