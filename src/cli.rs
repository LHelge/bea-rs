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
use crate::graph::{DepNode, DepNodeJson};
use crate::service;
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
    // Handle commands that don't need task data
    match &cli.command {
        Command::Init => return cmd_init(base, cli.json),
        Command::Completions { shell } => {
            clap_complete::generate(*shell, &mut Args::command(), "bea", &mut std::io::stdout());
            return Ok(());
        }
        Command::Version => {
            let version = env!("CARGO_PKG_VERSION");
            if cli.json {
                output(&serde_json::json!({ "version": version }), true)?;
            } else {
                println!("Bears {version}");
            }
            return Ok(());
        }
        Command::Mcp => unreachable!("MCP mode is handled in main"),
        _ => {}
    }

    // Load all tasks once for commands that need them
    let tasks = store::load_all(base).await?;

    match cli.command {
        Command::Create {
            title,
            priority,
            tag,
            depends_on,
            parent,
            body,
        } => cmd_create(
            base, &tasks, title, priority, tag, depends_on, parent, body, cli.json,
        ),
        Command::List {
            status,
            priority,
            tag,
            all,
        } => cmd_list(&tasks, status, priority, tag, all, cli.json),
        Command::Ready { tag, limit } => cmd_ready(&tasks, tag, limit, cli.json),
        Command::Show { id } => cmd_show(&tasks, &id, cli.json),
        Command::Update {
            id,
            status,
            priority,
            tag,
            assignee,
            body,
            title,
        } => cmd_update(
            base, &tasks, &id, status, priority, tag, assignee, body, title, cli.json,
        ),
        Command::Status { id, status } => cmd_status(base, &tasks, &id, status, cli.json),
        Command::Start { id } => cmd_status(base, &tasks, &id, Status::InProgress, cli.json),
        Command::Done { id } => cmd_status(base, &tasks, &id, Status::Done, cli.json),
        Command::Dep { command } => match command {
            DepCommand::Add { id, depends_on } => {
                cmd_dep_add(base, &tasks, &id, &depends_on, cli.json)
            }
            DepCommand::Remove { id, depends_on } => {
                cmd_dep_remove(base, &tasks, &id, &depends_on, cli.json)
            }
            DepCommand::Tree { id } => cmd_dep_tree(&tasks, &id, cli.json),
        },
        Command::Graph { all } => cmd_graph(&tasks, all, cli.json),
        Command::Cancel { id } => cmd_status(base, &tasks, &id, Status::Cancelled, cli.json),
        Command::Prune { done } => cmd_prune(base, &tasks, done, cli.json),
        Command::Delete { id } => cmd_delete(base, &tasks, &id, cli.json),
        Command::Search { query, all } => cmd_search(&tasks, &query, all, cli.json),
        Command::Edit { id } => cmd_edit(base, &tasks, &id, cli.json),
        // Already handled above
        Command::Init | Command::Completions { .. } | Command::Version | Command::Mcp => {
            unreachable!()
        }
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

/// Format priority with effective priority if it differs (e.g. "P3 → P1").
fn format_priority(own: &Priority, effective: Option<&Priority>) -> String {
    match effective {
        Some(eff) if eff < own => {
            format!("{} → {}", color_priority(own), color_priority(eff))
        }
        _ => color_priority(own),
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
        println!("Initialized bears in {}", dir.display());
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cmd_create(
    base: &Path,
    tasks: &HashMap<String, Task>,
    title: String,
    priority: Priority,
    tags: Vec<String>,
    depends_on: Vec<String>,
    parent: Option<String>,
    body: Option<String>,
    json: bool,
) -> Result<()> {
    let t = service::create_task(
        base,
        tasks,
        title,
        priority,
        tags,
        depends_on,
        parent,
        body.unwrap_or_default(),
    )?;

    if json {
        output(&t.summary(None), true)?;
    } else {
        println!("Created task {} — {}", t.id, t.title);
    }
    Ok(())
}

fn cmd_list(
    tasks: &HashMap<String, Task>,
    status: Option<Status>,
    priority: Option<Priority>,
    tag: Option<String>,
    all: bool,
    json: bool,
) -> Result<()> {
    let filtered = service::list_tasks(tasks, status, priority, tag.as_deref(), all);
    let eff = service::effective_priorities(tasks);

    if json {
        let summaries: Vec<_> = filtered.iter().map(|t| t.summary(eff.get(&t.id))).collect();
        output(&summaries, true)?;
    } else {
        if filtered.is_empty() {
            println!("No tasks found.");
        } else {
            for t in &filtered {
                println!(
                    "[{}] {} {} — {} [{}]",
                    color_id(&t.id),
                    format_priority(&t.priority, eff.get(&t.id)),
                    color_status(&t.status),
                    t.title,
                    color_tags(&t.tags)
                );
            }
        }
    }
    Ok(())
}

fn cmd_ready(
    tasks: &HashMap<String, Task>,
    tag: Option<String>,
    limit: Option<usize>,
    json: bool,
) -> Result<()> {
    let ready = service::list_ready(tasks, tag.as_deref(), limit);
    let eff = service::effective_priorities(tasks);

    if json {
        let summaries: Vec<_> = ready.iter().map(|t| t.summary(eff.get(&t.id))).collect();
        output(&summaries, true)?;
    } else {
        if ready.is_empty() {
            println!("No tasks ready.");
        } else {
            for t in &ready {
                println!(
                    "[{}] {} — {} [{}]",
                    color_id(&t.id),
                    format_priority(&t.priority, eff.get(&t.id)),
                    t.title,
                    color_tags(&t.tags)
                );
            }
        }
    }
    Ok(())
}

fn cmd_show(tasks: &HashMap<String, Task>, id: &str, json: bool) -> Result<()> {
    let t = service::get_task(tasks, id)?;
    let eff = service::effective_priorities(tasks);
    let ep = eff.get(&t.id);

    if json {
        output(&t.detail(ep), true)?;
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
            format_priority(&t.priority, ep)
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
    tasks: &HashMap<String, Task>,
    id: &str,
    status: Option<Status>,
    priority: Option<Priority>,
    tags: Option<Vec<String>>,
    assignee: Option<String>,
    body: Option<String>,
    title: Option<String>,
    json: bool,
) -> Result<()> {
    let t = service::update_task(
        base, tasks, id, status, priority, tags, assignee, body, title,
    )?;

    if json {
        output(&t.summary(None), true)?;
    } else {
        println!("Updated task {} — {}", t.id, t.title);
    }
    Ok(())
}

fn cmd_status(
    base: &Path,
    tasks: &HashMap<String, Task>,
    id: &str,
    status: Status,
    json: bool,
) -> Result<()> {
    let t = service::set_status(base, tasks, id, status)?;

    if json {
        output(&t.summary(None), true)?;
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

fn cmd_dep_add(
    base: &Path,
    tasks: &HashMap<String, Task>,
    id: &str,
    depends_on: &str,
    json: bool,
) -> Result<()> {
    let t = service::add_dependency(base, tasks, id, depends_on)?;

    if json {
        output(&t.summary(None), true)?;
    } else {
        println!("[{}] now depends on [{}]", id, depends_on);
    }
    Ok(())
}

fn cmd_dep_remove(
    base: &Path,
    tasks: &HashMap<String, Task>,
    id: &str,
    depends_on: &str,
    json: bool,
) -> Result<()> {
    let t = service::remove_dependency(base, tasks, id, depends_on)?;

    if json {
        output(&t.summary(None), true)?;
    } else {
        println!("[{}] no longer depends on [{}]", id, depends_on);
    }
    Ok(())
}

fn cmd_dep_tree(tasks: &HashMap<String, Task>, id: &str, json: bool) -> Result<()> {
    let graph = service::build_graph(tasks);
    let tree = graph
        .dep_tree(tasks, id)
        .ok_or_else(|| Error::TaskNotFound(id.into()))?;

    if json {
        let json_tree = DepNodeJson::from_dep_node(&tree);
        output(&json_tree, true)?;
    } else {
        let eff: HashMap<String, Priority> = tasks
            .keys()
            .map(|id| (id.clone(), graph.effective_priority(id, tasks)))
            .collect();
        println!(
            "Dependency tree for {}:\n",
            tree.task.title.if_supports_color(Stdout, |t| t.bold())
        );
        print_tree(&tree, "", true, tasks, &eff, true);
    }
    Ok(())
}

fn print_tree(
    node: &DepNode<'_>,
    prefix: &str,
    is_last: bool,
    tasks: &HashMap<String, Task>,
    eff: &HashMap<String, Priority>,
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
        format_priority(&t.priority, eff.get(&t.id)),
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
            eff,
            all,
        );
    }
}

fn cmd_graph(tasks: &HashMap<String, Task>, all: bool, json: bool) -> Result<()> {
    let graph = service::build_graph(tasks);

    if json {
        let adj = graph.adjacency_list();
        output(&adj, true)?;
        return Ok(());
    }

    if tasks.is_empty() {
        println!("No tasks.");
        return Ok(());
    }

    // Compute effective priorities
    let eff: HashMap<String, Priority> = tasks
        .keys()
        .map(|id| (id.clone(), graph.effective_priority(id, tasks)))
        .collect();

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
    roots.sort_by(|a, b| {
        eff.get(&a.id)
            .cmp(&eff.get(&b.id))
            .then(a.created.cmp(&b.created))
    });

    if roots.is_empty() {
        println!("No active tasks.");
        return Ok(());
    }

    println!("Dependency graph\n");
    for root in &roots {
        if let Some(tree) = graph.dep_tree(tasks, &root.id) {
            print_tree(&tree, "", true, tasks, &eff, all);
        }
    }
    Ok(())
}

fn cmd_prune(
    base: &Path,
    tasks: &HashMap<String, Task>,
    include_done: bool,
    json: bool,
) -> Result<()> {
    let deleted = service::prune_tasks(base, tasks, include_done)?;
    let summaries: Vec<_> = deleted.iter().map(|t| t.summary(None)).collect();

    if json {
        output(&summaries, true)?;
    } else {
        if deleted.is_empty() {
            println!("No tasks to prune.");
        } else {
            for s in &summaries {
                println!("Pruned {} — {}", s.id, s.title);
            }
        }
    }
    Ok(())
}

fn cmd_delete(base: &Path, tasks: &HashMap<String, Task>, id: &str, json: bool) -> Result<()> {
    let t = service::delete_task(base, tasks, id)?;

    if json {
        output(&t.summary(None), true)?;
    } else {
        println!("Deleted task {} — {}", t.id, t.title);
    }
    Ok(())
}

fn cmd_search(tasks: &HashMap<String, Task>, query: &str, all: bool, json: bool) -> Result<()> {
    let results = service::search_tasks(tasks, query, all);

    if json {
        let summaries: Vec<_> = results.iter().map(|t| t.summary(None)).collect();
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

fn cmd_edit(base: &Path, tasks: &HashMap<String, Task>, id: &str, json: bool) -> Result<()> {
    // Resolve prefix and load task
    let original = service::get_task(tasks, id)?;
    let path = store::find_task_path(base, &original.id)?;

    // Resolve editor
    let editor = std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| "vi".into());

    // Parse editor into executable and arguments (split on whitespace)
    let parts: Vec<&str> = editor.split_whitespace().collect();
    let (exe, args) = parts
        .split_first()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "EDITOR is empty"))?;

    let status = ProcessCommand::new(exe)
        .args(args)
        .arg(&path)
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
        output(&edited.summary(None), true)?;
    } else {
        println!("Edited task {} — {}", edited.id, edited.title);
    }
    Ok(())
}
