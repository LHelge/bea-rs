use clap::{Parser, Subcommand};
use clap_complete::Shell;

use crate::task::{Priority, Status};

#[derive(Parser)]
#[command(name = "bea", about = "bears — file-based task tracker")]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,

    /// Output JSON instead of human-readable text
    #[arg(long, global = true)]
    pub json: bool,
}

#[derive(Subcommand, PartialEq)]
pub enum Command {
    /// Initialize a new .tasks/ directory
    Init,

    /// Create a new task
    Create {
        /// Task title
        title: String,

        /// Priority (P0-P3)
        #[arg(long, default_value = "P2")]
        priority: Priority,

        /// Tags (comma-separated)
        #[arg(long, value_delimiter = ',')]
        tag: Vec<String>,

        /// Task IDs this depends on
        #[arg(long = "depends-on", value_delimiter = ',')]
        depends_on: Vec<String>,

        /// Parent task ID
        #[arg(long)]
        parent: Option<String>,

        /// Task body
        #[arg(long)]
        body: Option<String>,

        /// Create as an epic (high-level objective)
        #[arg(long)]
        epic: bool,
    },

    /// List tasks with optional filters
    List {
        /// Filter by status
        #[arg(long)]
        status: Option<Status>,

        /// Filter by priority
        #[arg(long)]
        priority: Option<Priority>,

        /// Filter by tag
        #[arg(long)]
        tag: Option<String>,

        /// Filter by parent epic ID
        #[arg(long)]
        epic: Option<String>,

        /// Include done and cancelled tasks
        #[arg(long, short = 'a')]
        all: bool,
    },

    /// Show tasks that are ready to work on
    Ready {
        /// Filter by tag
        #[arg(long)]
        tag: Option<String>,

        /// Filter by parent epic ID
        #[arg(long)]
        epic: Option<String>,

        /// Limit number of results
        #[arg(long)]
        limit: Option<usize>,
    },

    /// List all epics with progress
    Epics,

    /// Show a single task in detail
    Show {
        /// Task ID
        id: String,
    },

    /// Update task fields
    Update {
        /// Task ID
        id: String,

        /// New status
        #[arg(long)]
        status: Option<Status>,

        /// New priority
        #[arg(long)]
        priority: Option<Priority>,

        /// Set tags (comma-separated, replaces existing)
        #[arg(long, value_delimiter = ',')]
        tag: Option<Vec<String>>,

        /// Set assignee
        #[arg(long)]
        assignee: Option<String>,

        /// Set body
        #[arg(long)]
        body: Option<String>,

        /// New title
        #[arg(long)]
        title: Option<String>,
    },

    /// Set task status
    Status {
        /// Task ID
        id: String,
        /// New status
        status: Status,
    },

    /// Start a task (set status to in_progress)
    Start {
        /// Task ID
        id: String,
    },

    /// Complete a task (set status to done)
    Done {
        /// Task ID
        id: String,
    },

    /// Manage dependencies
    Dep {
        #[command(subcommand)]
        command: DepCommand,
    },

    /// Show dependency graph
    Graph {
        /// Include done and cancelled tasks
        #[arg(long, short = 'a')]
        all: bool,
    },

    /// Delete a task permanently
    Delete {
        /// Task ID
        id: String,
    },

    /// Search tasks by text
    Search {
        /// Search query
        query: String,

        /// Include done and cancelled tasks
        #[arg(long, short = 'a')]
        all: bool,
    },

    /// Cancel a task (set status to cancelled)
    Cancel {
        /// Task ID
        id: String,
    },

    /// Delete completed/cancelled tasks
    Prune {
        /// Also delete done tasks
        #[arg(long)]
        done: bool,
    },

    /// Open a task in $EDITOR for editing
    Edit {
        /// Task ID
        id: String,
    },

    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        shell: Shell,
    },

    /// Print version information
    Version,

    /// Start MCP server on stdio
    Mcp,
}

#[derive(Subcommand, PartialEq)]
pub enum DepCommand {
    /// Add a dependency
    Add {
        /// Task ID
        id: String,
        /// ID of task to depend on
        depends_on: String,
    },
    /// Remove a dependency
    Remove {
        /// Task ID
        id: String,
        /// ID of dependency to remove
        depends_on: String,
    },
    /// Show dependency tree
    Tree {
        /// Task ID
        id: String,
    },
}
