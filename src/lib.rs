//! `bears` — a file-based task tracker for AI agent workflows.
//!
//! Tasks are markdown files with YAML frontmatter in a `.bears/` directory.
//! This crate is the core library behind the `bea` CLI, MCP server, and TUI; it
//! is published so that an independent application can drive a bears repository
//! directly instead of shelling out to the binary.
//!
//! # Layering
//!
//! - [`store`] parses and writes the `.bears/` directory, including the archive.
//! - [`task`] defines [`Task`](task::Task) and the frontmatter format.
//! - [`graph`] computes the dependency graph, readiness, and effective priority.
//! - [`service`] is the business logic layer — create, update, reparent, archive,
//!   epic progress and auto-close. Most callers want this.
//! - [`scaffold`] writes coding-agent integration files (`CLAUDE.md`, skills, MCP config).
//!
//! There is no cache and no daemon: [`store::load_all`] re-reads the whole
//! directory, and mutating functions take the resulting map by reference.
//!
//! # Example
//!
//! ```no_run
//! use bears::{service, store};
//!
//! # async fn run() -> bears::error::Result<()> {
//! let base = std::path::Path::new(".");
//! let tasks = store::load_all(base).await?;
//!
//! for task in service::list_ready(&tasks, None, None, None) {
//!     println!("{} {} {}", task.id, task.priority, task.title);
//! }
//! # Ok(())
//! # }
//! ```

pub mod config;
pub mod error;
pub mod graph;
pub mod scaffold;
pub mod service;
pub mod store;
pub mod task;

pub use error::{Error, Result};
pub use task::{Priority, Status, Task, TaskType};
