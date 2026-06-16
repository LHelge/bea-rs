use clap::{Parser, Subcommand};
use clap_complete::Shell;

use crate::task::{Priority, Status};

#[derive(Parser)]
#[command(
    name = "bea",
    about = "Bears 🐻🐻 - A file-based task tracker CLI and MCP server for AI agent workflows"
)]
#[command(version, propagate_version = true)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,

    /// Output JSON instead of human-readable text
    #[arg(long, global = true)]
    pub json: bool,
}

#[derive(Subcommand, PartialEq)]
pub enum Command {
    /// Initialize a new .bears/ directory
    Init {
        /// Scaffold Claude Code integration files (CLAUDE.md, .mcp.json)
        #[arg(long)]
        claude: bool,

        /// Scaffold GitHub Copilot integration files (.github/copilot-instructions.md, .github/mcp.json)
        #[arg(long)]
        copilot: bool,

        /// Scaffold OpenAI Codex integration files (AGENTS.md)
        #[arg(long)]
        codex: bool,

        /// Overwrite existing agent files without prompting
        #[arg(long)]
        force: bool,
    },

    /// Scaffold coding-agent integration files (does not create .bears/)
    Agent {
        /// Which files to scaffold
        #[arg(value_enum)]
        category: AgentCategory,

        /// Scaffold Claude Code files
        #[arg(long)]
        claude: bool,

        /// Scaffold GitHub Copilot files
        #[arg(long)]
        copilot: bool,

        /// Scaffold OpenAI Codex files
        #[arg(long)]
        codex: bool,

        /// Overwrite existing files without prompting
        #[arg(long)]
        force: bool,

        /// Append to an existing instruction file instead of overwriting
        /// (only valid for the `instructions` and `all` categories)
        #[arg(long, conflicts_with = "force")]
        append: bool,
    },

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

        /// Show archived tasks instead of active tasks
        #[arg(long, conflicts_with_all = ["status", "priority", "tag", "epic", "all"])]
        archived: bool,
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

        /// Output subtasks in execution order as markdown
        #[arg(long)]
        plan: bool,
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

        /// Set parent epic ID (use empty string "" to clear)
        #[arg(long)]
        parent: Option<String>,
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

    /// Delete completed/cancelled tasks [deprecated: prefer `bea archive`]
    Prune {
        /// Also delete done tasks
        #[arg(long)]
        done: bool,
    },

    /// Archive a task (or sweep all archivable tasks if no ID given)
    Archive {
        /// Task ID or prefix to archive (omit to sweep all done/cancelled tasks)
        id: Option<String>,
    },

    /// Restore a task from the archive
    Restore {
        /// Task ID or prefix to restore
        id: String,
    },

    /// Show archived tasks as a chronological log (most-recent-first)
    Log {
        /// Limit number of results
        #[arg(long)]
        limit: Option<usize>,
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

    /// Start MCP server on stdio
    Mcp,

    /// Launch interactive TUI
    Tui,
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

/// Which subset of agent integration files `bea agent` should scaffold.
#[derive(clap::ValueEnum, Clone, Copy, PartialEq, Eq, Debug)]
pub enum AgentCategory {
    /// Top-level instruction file only (CLAUDE.md / AGENTS.md / copilot-instructions.md)
    Instructions,
    /// Skill, reference, and planner-agent files plus MCP registration
    Skills,
    /// Everything (same files as `bea init`)
    All,
}

impl From<AgentCategory> for crate::scaffold::Category {
    fn from(c: AgentCategory) -> Self {
        match c {
            AgentCategory::Instructions => crate::scaffold::Category::Instructions,
            AgentCategory::Skills => crate::scaffold::Category::Skills,
            AgentCategory::All => crate::scaffold::Category::All,
        }
    }
}
