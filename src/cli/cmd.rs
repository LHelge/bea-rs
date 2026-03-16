use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command as ProcessCommand;

use chrono::Utc;
use owo_colors::OwoColorize;
use owo_colors::Stream::Stdout;
use owo_colors::Style;

use crate::error::{Error, Result};
use crate::graph::{DepNode, DepNodeJson};
use crate::service;
use crate::store;
use crate::task::{self, Priority, Status, Task, TaskType};

use super::{color_id, color_priority, color_status, color_tags, format_priority, output};

pub fn cmd_init(base: &Path, json: bool) -> Result<()> {
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
pub fn cmd_create(
    base: &Path,
    tasks: &HashMap<String, Task>,
    title: String,
    priority: Priority,
    tags: Vec<String>,
    depends_on: Vec<String>,
    parent: Option<String>,
    body: Option<String>,
    epic: bool,
    json: bool,
) -> Result<()> {
    let task_type = if epic { TaskType::Epic } else { TaskType::Task };
    let t = service::create_task(
        base,
        tasks,
        title,
        priority,
        tags,
        depends_on,
        parent,
        body.unwrap_or_default(),
        task_type,
    )?;

    if json {
        output(&t.summary(None), true)?;
    } else {
        println!("Created task {} — {}", t.id, t.title);
    }
    Ok(())
}

pub fn cmd_list(
    tasks: &HashMap<String, Task>,
    status: Option<Status>,
    priority: Option<Priority>,
    tag: Option<String>,
    epic: Option<String>,
    all: bool,
    json: bool,
) -> Result<()> {
    let filtered = service::list_tasks(
        tasks,
        status,
        priority,
        tag.as_deref(),
        all,
        epic.as_deref(),
    );
    let eff = service::effective_priorities(tasks);

    if json {
        let summaries: Vec<_> = filtered.iter().map(|t| t.summary(eff.get(&t.id))).collect();
        output(&summaries, true)?;
    } else {
        if filtered.is_empty() {
            println!("No tasks found.");
        } else {
            for t in &filtered {
                if t.task_type.is_epic() {
                    let p = service::epic_progress(tasks, &t.id);
                    println!(
                        "[{}] {} {} {} {} [{}/{}] [{}]",
                        color_id(&t.id),
                        format_priority(&t.priority, eff.get(&t.id)),
                        color_status(&t.status),
                        "Epic:".if_supports_color(Stdout, |t| t.bright_magenta()),
                        t.title,
                        p.done,
                        p.total,
                        color_tags(&t.tags),
                    );
                } else {
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
    }
    Ok(())
}

pub fn cmd_ready(
    tasks: &HashMap<String, Task>,
    tag: Option<String>,
    epic: Option<String>,
    limit: Option<usize>,
    json: bool,
) -> Result<()> {
    let ready = service::list_ready(tasks, tag.as_deref(), limit, epic.as_deref());
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

pub fn cmd_epics(tasks: &HashMap<String, Task>, json: bool) -> Result<()> {
    let mut epics: Vec<&Task> = tasks.values().filter(|t| t.task_type.is_epic()).collect();
    epics.sort_by(|a, b| a.priority.cmp(&b.priority).then(a.created.cmp(&b.created)));

    if json {
        let out: Vec<_> = epics
            .iter()
            .map(|t| t.epic_summary(service::epic_progress(tasks, &t.id)))
            .collect();
        output(&out, true)?;
    } else if epics.is_empty() {
        println!("No epics found.");
    } else {
        for t in &epics {
            let p = service::epic_progress(tasks, &t.id);
            let prefix = "Epic:"
                .if_supports_color(Stdout, |t| t.bright_magenta())
                .to_string();
            println!(
                "[{}] {} {} {} {} [{}/{}] [{}]",
                color_id(&t.id),
                color_priority(&t.priority),
                color_status(&t.status),
                prefix,
                t.title,
                p.done,
                p.total,
                color_tags(&t.tags),
            );
        }
    }
    Ok(())
}

pub fn cmd_show(tasks: &HashMap<String, Task>, id: &str, json: bool) -> Result<()> {
    let t = service::get_task(tasks, id)?;
    let eff = service::effective_priorities(tasks);
    let ep = eff.get(&t.id);

    if json {
        output(&t.detail(ep), true)?;
    } else {
        if t.task_type.is_epic() {
            let p = service::epic_progress(tasks, &t.id);
            println!(
                "[{}] {} {} [{}/{}]",
                color_id(&t.id),
                "Epic:".if_supports_color(Stdout, |t| t.bright_magenta()),
                t.title.if_supports_color(Stdout, |t| t.bold()),
                p.done,
                p.total,
            );
        } else {
            println!(
                "[{}] {}",
                color_id(&t.id),
                t.title.if_supports_color(Stdout, |t| t.bold())
            );
        }
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
pub fn cmd_update(
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

pub fn cmd_status(
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

pub fn cmd_dep_add(
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

pub fn cmd_dep_remove(
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

pub fn cmd_dep_tree(tasks: &HashMap<String, Task>, id: &str, json: bool) -> Result<()> {
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
    let title = if blocked {
        t.title
            .if_supports_color(Stdout, |s| s.dimmed())
            .to_string()
    } else {
        t.title.clone()
    };

    let epic_prefix = if t.task_type.is_epic() {
        format!(
            "{} ",
            "Epic:".if_supports_color(Stdout, |t| t.bright_magenta())
        )
    } else {
        String::new()
    };

    println!(
        "{prefix}{connector}[{}] {epic_prefix}{title} [{}] ({}){cycle_suffix}",
        color_id(&t.id),
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

pub fn cmd_graph(tasks: &HashMap<String, Task>, all: bool, json: bool) -> Result<()> {
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

pub fn cmd_prune(
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

pub fn cmd_delete(base: &Path, tasks: &HashMap<String, Task>, id: &str, json: bool) -> Result<()> {
    let t = service::delete_task(base, tasks, id)?;

    if json {
        output(&t.summary(None), true)?;
    } else {
        println!("Deleted task {} — {}", t.id, t.title);
    }
    Ok(())
}

pub fn cmd_search(tasks: &HashMap<String, Task>, query: &str, all: bool, json: bool) -> Result<()> {
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

pub fn cmd_edit(base: &Path, tasks: &HashMap<String, Task>, id: &str, json: bool) -> Result<()> {
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
