use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command as ProcessCommand;

use chrono::Utc;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use owo_colors::OwoColorize;
use owo_colors::Stream::Stdout;
use owo_colors::Style;
use serde::Serialize;

use crate::error::{Error, Result};
use crate::graph::{DepNode, DepNodeJson, Graph};
use crate::store;
use crate::task::{self, Priority, Status, Task};

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

        /// Include done and cancelled tasks
        #[arg(long, short = 'a')]
        all: bool,
    },

    /// Show tasks that are ready to work on
    Ready {
        /// Filter by tag
        #[arg(long)]
        tag: Option<String>,

        /// Limit number of results
        #[arg(long)]
        limit: Option<usize>,
    },

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

pub async fn run(cli: Args, base: &Path) -> Result<()> {
    match cli.command {
        Command::Init => cmd_init(base, cli.json),
        Command::Create {
            title,
            priority,
            tag,
            depends_on,
            parent,
            body,
        } => {
            cmd_create(
                base, title, priority, tag, depends_on, parent, body, cli.json,
            )
            .await
        }
        Command::List {
            status,
            priority,
            tag,
            all,
        } => cmd_list(base, status, priority, tag, all, cli.json).await,
        Command::Ready { tag, limit } => cmd_ready(base, tag, limit, cli.json).await,
        Command::Show { id } => cmd_show(base, &id, cli.json),
        Command::Update {
            id,
            status,
            priority,
            tag,
            assignee,
            body,
            title,
        } => cmd_update(
            base, &id, status, priority, tag, assignee, body, title, cli.json,
        ),
        Command::Status { id, status } => cmd_status(base, &id, status, cli.json),
        Command::Start { id } => cmd_status(base, &id, Status::InProgress, cli.json),
        Command::Done { id } => cmd_status(base, &id, Status::Done, cli.json),
        Command::Dep { command } => match command {
            DepCommand::Add { id, depends_on } => {
                cmd_dep_add(base, &id, &depends_on, cli.json).await
            }
            DepCommand::Remove { id, depends_on } => {
                cmd_dep_remove(base, &id, &depends_on, cli.json)
            }
            DepCommand::Tree { id } => cmd_dep_tree(base, &id, cli.json).await,
        },
        Command::Graph { all } => cmd_graph(base, all, cli.json).await,
        Command::Cancel { id } => cmd_status(base, &id, Status::Cancelled, cli.json),
        Command::Prune { done } => cmd_prune(base, done, cli.json).await,
        Command::Delete { id } => cmd_delete(base, &id, cli.json),
        Command::Search { query, all } => cmd_search(base, &query, all, cli.json).await,
        Command::Edit { id } => cmd_edit(base, &id, cli.json),
        Command::Completions { shell } => {
            clap_complete::generate(shell, &mut Args::command(), "bea", &mut std::io::stdout());
            Ok(())
        }
        Command::Version => {
            let version = env!("CARGO_PKG_VERSION");
            if cli.json {
                output(&serde_json::json!({ "version": version }), true)?;
            } else {
                println!("Bears {version}");
            }
            Ok(())
        }
        Command::Mcp => unreachable!("MCP mode is handled in main"),
    }
}

fn color_priority(p: &Priority) -> String {
    match p {
        Priority::P0 => {
            let style = Style::new().bold().red();
            p.if_supports_color(Stdout, |t| t.style(style)).to_string()
        }
        Priority::P1 => p.if_supports_color(Stdout, |t| t.red()).to_string(),
        Priority::P2 => p.if_supports_color(Stdout, |t| t.yellow()).to_string(),
        Priority::P3 => p.to_string(),
    }
}

fn color_status(s: &Status) -> String {
    match s {
        Status::Open => s.to_string(),
        Status::InProgress => s.if_supports_color(Stdout, |t| t.cyan()).to_string(),
        Status::Done => s.if_supports_color(Stdout, |t| t.green()).to_string(),
        Status::Blocked => s.if_supports_color(Stdout, |t| t.red()).to_string(),
        Status::Cancelled => s.if_supports_color(Stdout, |t| t.dimmed()).to_string(),
    }
}

fn color_id(id: &str) -> String {
    id.if_supports_color(Stdout, |t| t.dimmed()).to_string()
}

fn color_tags(tags: &[String]) -> String {
    let joined = tags.join(", ");
    joined.if_supports_color(Stdout, |t| t.dimmed()).to_string()
}

fn output<T: Serialize>(value: &T, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(value)?);
    }
    Ok(())
}

fn cmd_init(base: &Path, json: bool) -> Result<()> {
    let dir = store::init(base)?;
    if json {
        output(
            &serde_json::json!({ "path": dir.display().to_string() }),
            true,
        )?;
    } else {
        println!("Initialized .tasks/ directory at {}", dir.display());
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn cmd_create(
    base: &Path,
    title: String,
    priority: Priority,
    tags: Vec<String>,
    depends_on: Vec<String>,
    parent: Option<String>,
    body: Option<String>,
    json: bool,
) -> Result<()> {
    let tasks = store::load_all(base).await?;
    let existing_ids: HashSet<String> = tasks.keys().cloned().collect();
    let id = task::generate_id(&existing_ids);

    let mut t = Task::new(id, title, priority);
    t.tags = tags;
    t.depends_on = depends_on;
    t.parent = parent;
    if let Some(body) = body {
        t.body = body;
    }

    store::save(base, &t)?;

    if json {
        output(&task_summary(&t), true)?;
    } else {
        println!("Created task {} — {}", t.id, t.title);
    }
    Ok(())
}

async fn cmd_list(
    base: &Path,
    status: Option<Status>,
    priority: Option<Priority>,
    tag: Option<String>,
    all: bool,
    json: bool,
) -> Result<()> {
    let tasks = store::load_all(base).await?;
    let mut filtered: Vec<&Task> = tasks
        .values()
        .filter(|t| {
            if status.is_some() || all {
                true
            } else {
                !matches!(t.status, Status::Done | Status::Cancelled)
            }
        })
        .filter(|t| status.as_ref().is_none_or(|s| t.status == *s))
        .filter(|t| priority.as_ref().is_none_or(|p| t.priority == *p))
        .filter(|t| {
            tag.as_ref()
                .is_none_or(|tag| t.tags.iter().any(|tt| tt == tag))
        })
        .collect();
    filtered.sort_by(|a, b| a.priority.cmp(&b.priority).then(a.created.cmp(&b.created)));

    if json {
        let summaries: Vec<_> = filtered.iter().map(|t| task_summary(t)).collect();
        output(&summaries, true)?;
    } else {
        if filtered.is_empty() {
            println!("No tasks found.");
        } else {
            for t in &filtered {
                println!(
                    "[{}] {} {} — {} [{}]",
                    color_id(&t.id),
                    color_priority(&t.priority),
                    color_status(&t.status),
                    t.title,
                    color_tags(&t.tags)
                );
            }
        }
    }
    Ok(())
}

async fn cmd_ready(
    base: &Path,
    tag: Option<String>,
    limit: Option<usize>,
    json: bool,
) -> Result<()> {
    let tasks = store::load_all(base).await?;
    let graph = Graph::build(&tasks);
    let ready = graph.ready(&tasks, tag.as_deref(), limit);

    if json {
        let summaries: Vec<_> = ready.iter().map(|t| task_summary(t)).collect();
        output(&summaries, true)?;
    } else {
        if ready.is_empty() {
            println!("No tasks ready.");
        } else {
            for t in &ready {
                println!(
                    "[{}] {} — {} [{}]",
                    color_id(&t.id),
                    color_priority(&t.priority),
                    t.title,
                    color_tags(&t.tags)
                );
            }
        }
    }
    Ok(())
}

fn cmd_show(base: &Path, id: &str, json: bool) -> Result<()> {
    let t = store::load_one(base, id)?;

    if json {
        let mut s = task_summary(&t);
        s["body"] = serde_json::Value::String(t.body.clone());
        s["depends_on"] = serde_json::json!(t.depends_on);
        s["parent"] = serde_json::json!(t.parent);
        s["assignee"] = serde_json::json!(t.assignee);
        s["created"] = serde_json::json!(t.created);
        s["updated"] = serde_json::json!(t.updated);
        output(&s, true)?;
    } else {
        println!(
            "[{}] {}",
            color_id(&t.id),
            t.title.if_supports_color(Stdout, |t| t.bold())
        );
        println!(
            "{} {}",
            "Status:  ".if_supports_color(Stdout, |t| t.bold()),
            color_status(&t.status)
        );
        println!(
            "{} {}",
            "Priority:".if_supports_color(Stdout, |t| t.bold()),
            color_priority(&t.priority)
        );
        println!(
            "{} {}",
            "Tags:    ".if_supports_color(Stdout, |t| t.bold()),
            if t.tags.is_empty() {
                "—".to_string()
            } else {
                color_tags(&t.tags)
            }
        );
        if !t.depends_on.is_empty() {
            println!(
                "{} {}",
                "Deps:    ".if_supports_color(Stdout, |t| t.bold()),
                t.depends_on
                    .iter()
                    .map(|d| color_id(d))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        if let Some(ref parent) = t.parent {
            println!(
                "{} {}",
                "Parent:  ".if_supports_color(Stdout, |t| t.bold()),
                color_id(parent)
            );
        }
        if !t.assignee.is_empty() {
            println!(
                "{} {}",
                "Assignee:".if_supports_color(Stdout, |t| t.bold()),
                t.assignee
            );
        }
        println!(
            "{} {}",
            "Created: ".if_supports_color(Stdout, |t| t.bold()),
            t.created
        );
        println!(
            "{} {}",
            "Updated: ".if_supports_color(Stdout, |t| t.bold()),
            t.updated
        );
        if !t.body.is_empty() {
            println!("\n{}", t.body);
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cmd_update(
    base: &Path,
    id: &str,
    status: Option<Status>,
    priority: Option<Priority>,
    tags: Option<Vec<String>>,
    assignee: Option<String>,
    body: Option<String>,
    title: Option<String>,
    json: bool,
) -> Result<()> {
    let mut t = store::load_one(base, id)?;

    if let Some(s) = status {
        t.status = s;
    }
    if let Some(p) = priority {
        t.priority = p;
    }
    if let Some(tags) = tags {
        t.tags = tags;
    }
    if let Some(a) = assignee {
        t.assignee = a;
    }
    if let Some(b) = body {
        t.body = b;
    }
    if let Some(title) = title {
        t.title = title;
    }
    t.updated = Utc::now();

    store::save(base, &t)?;

    if json {
        output(&task_summary(&t), true)?;
    } else {
        println!("Updated task {} — {}", t.id, t.title);
    }
    Ok(())
}

fn cmd_status(base: &Path, id: &str, status: Status, json: bool) -> Result<()> {
    let mut t = store::load_one(base, id)?;
    t.status = status;
    t.updated = Utc::now();
    store::save(base, &t)?;

    if json {
        output(&task_summary(&t), true)?;
    } else {
        println!(
            "[{}] {} → {}",
            color_id(&t.id),
            t.title,
            color_status(&t.status)
        );
    }
    Ok(())
}

async fn cmd_dep_add(base: &Path, id: &str, depends_on: &str, json: bool) -> Result<()> {
    // Verify both tasks exist
    let _ = store::load_one(base, depends_on)?;
    let mut t = store::load_one(base, id)?;

    // Check for cycles
    let tasks = store::load_all(base).await?;
    let graph = Graph::build(&tasks);
    if graph.would_cycle(id, depends_on) {
        return Err(Error::CycleDetected {
            from: id.into(),
            to: depends_on.into(),
        });
    }

    if !t.depends_on.contains(&depends_on.to_string()) {
        t.depends_on.push(depends_on.to_string());
        t.updated = Utc::now();
        store::save(base, &t)?;
    }

    if json {
        output(&task_summary(&t), true)?;
    } else {
        println!("[{}] now depends on [{}]", id, depends_on);
    }
    Ok(())
}

fn cmd_dep_remove(base: &Path, id: &str, depends_on: &str, json: bool) -> Result<()> {
    let mut t = store::load_one(base, id)?;
    t.depends_on.retain(|d| d != depends_on);
    t.updated = Utc::now();
    store::save(base, &t)?;

    if json {
        output(&task_summary(&t), true)?;
    } else {
        println!("[{}] no longer depends on [{}]", id, depends_on);
    }
    Ok(())
}

async fn cmd_dep_tree(base: &Path, id: &str, json: bool) -> Result<()> {
    let tasks = store::load_all(base).await?;
    let graph = Graph::build(&tasks);
    let tree = graph
        .dep_tree(&tasks, id)
        .ok_or_else(|| Error::TaskNotFound(id.into()))?;

    if json {
        let json_tree = DepNodeJson::from_dep_node(&tree);
        output(&json_tree, true)?;
    } else {
        println!(
            "Dependency tree for {}:\n",
            tree.task.title.if_supports_color(Stdout, |t| t.bold())
        );
        print_tree(&tree, "", true, &tasks, true);
    }
    Ok(())
}

fn print_tree(
    node: &DepNode<'_>,
    prefix: &str,
    is_last: bool,
    tasks: &HashMap<String, Task>,
    all: bool,
) {
    let connector = if prefix.is_empty() {
        ""
    } else if is_last {
        "└── "
    } else {
        "├── "
    };

    let t = node.task;
    let blocked = matches!(t.status, Status::Open | Status::InProgress)
        && t.depends_on.iter().any(|dep_id| {
            tasks
                .get(dep_id)
                .is_some_and(|dep| dep.status != Status::Done)
        });
    let cycle_suffix = if node.cycle {
        let style = Style::new().bold().yellow();
        format!(
            " {}",
            "[CYCLE]".if_supports_color(Stdout, |s| s.style(style))
        )
    } else {
        String::new()
    };
    let blocked_suffix = if blocked {
        let style = Style::new().bold().red();
        format!(
            " {}",
            "[BLOCKED]".if_supports_color(Stdout, |s| s.style(style))
        )
    } else {
        String::new()
    };

    println!(
        "{prefix}{connector}[{}] {} [{}] ({}){blocked_suffix}{cycle_suffix}",
        color_id(&t.id),
        t.title,
        color_priority(&t.priority),
        color_status(&t.status),
    );

    let child_prefix = if is_last {
        format!("{prefix}    ")
    } else {
        format!("{prefix}│   ")
    };
    let visible_children: Vec<_> = node
        .children
        .iter()
        .filter(|c| all || !matches!(c.task.status, Status::Done | Status::Cancelled))
        .collect();
    for (i, child) in visible_children.iter().enumerate() {
        print_tree(
            child,
            &child_prefix,
            i == visible_children.len() - 1,
            tasks,
            all,
        );
    }
}

async fn cmd_graph(base: &Path, all: bool, json: bool) -> Result<()> {
    let tasks = store::load_all(base).await?;
    let graph = Graph::build(&tasks);

    if json {
        let adj = graph.adjacency_list();
        output(&adj, true)?;
        return Ok(());
    }

    if tasks.is_empty() {
        println!("No tasks.");
        return Ok(());
    }

    // Roots: tasks that no other task depends on
    let all_deps: HashSet<&str> = tasks
        .values()
        .flat_map(|t| t.depends_on.iter().map(String::as_str))
        .collect();
    let mut roots: Vec<&Task> = tasks
        .values()
        .filter(|t| !all_deps.contains(t.id.as_str()))
        .filter(|t| all || !matches!(t.status, Status::Done | Status::Cancelled))
        .collect();
    roots.sort_by(|a, b| a.priority.cmp(&b.priority).then(a.created.cmp(&b.created)));

    if roots.is_empty() {
        println!("No active tasks.");
        return Ok(());
    }

    println!("Dependency graph\n");
    for root in &roots {
        if let Some(tree) = graph.dep_tree(&tasks, &root.id) {
            print_tree(&tree, "", true, &tasks, all);
        }
    }
    Ok(())
}

async fn cmd_prune(base: &Path, include_done: bool, json: bool) -> Result<()> {
    let tasks = store::load_all(base).await?;
    let to_delete: Vec<&Task> = tasks
        .values()
        .filter(|t| t.status == Status::Cancelled || (include_done && t.status == Status::Done))
        .collect();

    let summaries: Vec<_> = to_delete.iter().map(|t| task_summary(t)).collect();
    for t in &to_delete {
        store::delete(base, &t.id)?;
    }

    if json {
        output(&summaries, true)?;
    } else {
        if to_delete.is_empty() {
            println!("No tasks to prune.");
        } else {
            for s in &summaries {
                println!(
                    "Pruned {} — {}",
                    s["id"].as_str().unwrap_or_default(),
                    s["title"].as_str().unwrap_or_default()
                );
            }
        }
    }
    Ok(())
}

fn cmd_delete(base: &Path, id: &str, json: bool) -> Result<()> {
    let t = store::load_one(base, id)?;
    store::delete(base, id)?;

    if json {
        output(&task_summary(&t), true)?;
    } else {
        println!("Deleted task {} — {}", t.id, t.title);
    }
    Ok(())
}

async fn cmd_search(base: &Path, query: &str, all: bool, json: bool) -> Result<()> {
    let tasks = store::load_all(base).await?;
    let query_lower = query.to_lowercase();
    let mut results: Vec<&Task> = tasks
        .values()
        .filter(|t| all || !matches!(t.status, Status::Done | Status::Cancelled))
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

    if json {
        let summaries: Vec<_> = results.iter().map(|t| task_summary(t)).collect();
        output(&summaries, true)?;
    } else {
        if results.is_empty() {
            println!("No tasks matching \"{query}\".");
        } else {
            for t in &results {
                println!(
                    "[{}] {} {} — {} [{}]",
                    color_id(&t.id),
                    color_priority(&t.priority),
                    color_status(&t.status),
                    t.title,
                    color_tags(&t.tags)
                );
            }
        }
    }
    Ok(())
}

fn cmd_edit(base: &Path, id: &str, json: bool) -> Result<()> {
    // Validate task exists
    let original = store::load_one(base, id)?;
    let path = store::find_task_path(base, &original.id)?;

    // Resolve editor
    let editor = std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| "vi".into());

    // Open editor — use sh -c so $EDITOR can contain arguments
    let path_str = path.to_string_lossy();
    let status = ProcessCommand::new("sh")
        .arg("-c")
        .arg(format!("{editor} \"{path_str}\""))
        .status()
        .map_err(Error::Io)?;

    if !status.success() {
        eprintln!("Editor exited with non-zero status, aborting.");
        return Ok(());
    }

    // Re-read and validate
    let content = std::fs::read_to_string(&path).map_err(Error::Io)?;
    let edited = match task::parse_task(&content) {
        Ok(t) => t,
        Err(_) => {
            eprintln!("Invalid frontmatter after edit. File left on disk — fix and retry.");
            return Ok(());
        }
    };

    // Check if anything changed
    if task::render_task(&original) == task::render_task(&edited) {
        if !json {
            println!("No changes.");
        }
        return Ok(());
    }

    // Save with updated timestamp (and handle slug rename if title changed)
    let mut edited = edited;
    edited.updated = Utc::now();
    store::save(base, &edited)?;

    if json {
        output(&task_summary(&edited), true)?;
    } else {
        println!("Edited task {} — {}", edited.id, edited.title);
    }
    Ok(())
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
