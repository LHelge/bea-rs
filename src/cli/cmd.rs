use std::collections::{HashMap, HashSet};
use std::path::Path;

use jiff::Timestamp;
use owo_colors::OwoColorize;
use owo_colors::Stream::Stdout;
use owo_colors::Style;

use bears::error::{Error, Result};
use bears::graph::{DepNode, DepNodeJson};
use bears::scaffold;
use bears::service;
use bears::snapshot;
use bears::store;
use bears::task::{self, Priority, Status, Task, TaskType};

use super::SnapshotFormat;
use super::{
    color_id, color_priority, color_status, color_tags, format_priority, id_cell, id_width, output,
};

/// Map an affirmative-prompt response to a boolean. Accepts `y`/`yes`
/// (case-insensitive); anything else (including empty) is treated as No.
fn parse_yes(line: &str) -> bool {
    matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

/// Resolve the write policy for a scaffold run when the user passed neither
/// `--force` nor `--append`.
///
/// - Nothing on disk yet → `Force` (plain create).
/// - Files exist but stdin isn't a terminal (CI/pipe) → `SkipExisting` (never
///   blocks waiting for input).
/// - Files exist and we have a TTY → prompt once; `y`/`yes` → `Force`, else
///   `SkipExisting`.
fn resolve_overwrite_policy(any_exists: bool) -> Result<scaffold::WritePolicy> {
    use std::io::{IsTerminal, Write};

    if !any_exists {
        return Ok(scaffold::WritePolicy::Force);
    }
    if !std::io::stdin().is_terminal() {
        return Ok(scaffold::WritePolicy::SkipExisting);
    }

    eprint!(
        "This directory already contains agent instructions and/or skills. \
         Overwrite them? (y/N) "
    );
    std::io::stderr().flush().ok();

    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    Ok(if parse_yes(&line) {
        scaffold::WritePolicy::Force
    } else {
        scaffold::WritePolicy::SkipExisting
    })
}

/// Collect the requested harness labels from the per-harness flags.
fn collect_labels(claude: bool, copilot: bool, codex: bool) -> Vec<&'static str> {
    let mut labels: Vec<&str> = Vec::new();
    if claude {
        labels.push("claude");
    }
    if copilot {
        labels.push("copilot");
    }
    if codex {
        labels.push("codex");
    }
    labels
}

pub fn cmd_init(
    base: &Path,
    claude: bool,
    copilot: bool,
    codex: bool,
    force: bool,
    json: bool,
) -> Result<()> {
    let dir = store::init(base)?;

    let labels = collect_labels(claude, copilot, codex);

    let policy = if force {
        scaffold::WritePolicy::Force
    } else {
        let any_exists = scaffold::category_targets(base, &labels, scaffold::Category::All)
            .iter()
            .any(|p| p.exists());
        resolve_overwrite_policy(any_exists)?
    };

    let scaffolded = scaffold::scaffold_category(base, &labels, scaffold::Category::All, policy)?;

    if json {
        let paths: Vec<String> = std::iter::once(dir.display().to_string())
            .chain(scaffolded.iter().map(|p| p.display().to_string()))
            .collect();
        output(&serde_json::json!({
            "path": dir.display().to_string(),
            "scaffolded": paths[1..],
        }))?;
    } else {
        println!("Initialized bears in {}", dir.display());
        for p in &scaffolded {
            println!("  wrote {}", p.display());
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn cmd_agent_init(
    base: &Path,
    category: crate::cli::AgentCategory,
    claude: bool,
    copilot: bool,
    codex: bool,
    force: bool,
    append: bool,
    json: bool,
) -> Result<()> {
    use crate::cli::AgentCategory;

    // `--append` only makes sense where there is an instruction file to append to.
    if append && category == AgentCategory::Skills {
        return Err(Error::InvalidUsage(
            "--append is only valid for `instructions` or `all` (there is no instruction file to append to for `skills`)"
                .to_string(),
        ));
    }

    let labels = collect_labels(claude, copilot, codex);
    if labels.is_empty() {
        return Err(Error::InvalidUsage(
            "select at least one harness: --claude, --copilot, or --codex".to_string(),
        ));
    }

    let cat: scaffold::Category = category.into();

    let policy = if force {
        scaffold::WritePolicy::Force
    } else if append {
        scaffold::WritePolicy::Append
    } else {
        let any_exists = scaffold::category_targets(base, &labels, cat)
            .iter()
            .any(|p| p.exists());
        resolve_overwrite_policy(any_exists)?
    };

    let scaffolded = scaffold::scaffold_category(base, &labels, cat, policy)?;

    if json {
        let paths: Vec<String> = scaffolded.iter().map(|p| p.display().to_string()).collect();
        output(&serde_json::json!({ "scaffolded": paths }))?;
    } else if scaffolded.is_empty() {
        println!("Nothing to do (target files already exist; use --force to overwrite).");
    } else {
        for p in &scaffolded {
            println!("  wrote {}", p.display());
        }
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
        output(&t.summary(None))?;
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
    // `--all` is the CLI's "show me everything", proposals included.
    let filtered = service::list_tasks(
        tasks,
        status,
        priority,
        tag.as_deref(),
        all,
        all,
        epic.as_deref(),
    );
    let eff = service::effective_priorities(tasks);

    if json {
        let summaries: Vec<_> = filtered.iter().map(|t| t.summary(eff.get(&t.id))).collect();
        output(&summaries)?;
    } else {
        if filtered.is_empty() {
            println!("No tasks found.");
        } else {
            let w = id_width(filtered.iter().map(|t| t.id.as_str()));
            for t in &filtered {
                if t.task_type.is_epic() {
                    let p = service::epic_progress(tasks, &t.id);
                    println!(
                        "{} {} {} {} {} [{}/{}] [{}]",
                        id_cell(&t.id, w),
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
                        "{} {} {} — {} [{}]",
                        id_cell(&t.id, w),
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
        output(&summaries)?;
    } else {
        if ready.is_empty() {
            println!("No tasks ready.");
        } else {
            let w = id_width(ready.iter().map(|t| t.id.as_str()));
            for t in &ready {
                println!(
                    "{} {} — {} [{}]",
                    id_cell(&t.id, w),
                    format_priority(&t.priority, eff.get(&t.id)),
                    t.title,
                    color_tags(&t.tags)
                );
            }
        }
    }
    Ok(())
}

/// List the review queue (`bea review` with no ID).
pub fn cmd_review_queue(
    tasks: &HashMap<String, Task>,
    tag: Option<&str>,
    epic: Option<&str>,
    limit: Option<usize>,
    json: bool,
) -> Result<()> {
    let queue = service::list_review(tasks, tag, limit, epic);
    let eff = service::effective_priorities(tasks);

    if json {
        let summaries: Vec<_> = queue.iter().map(|t| t.summary(eff.get(&t.id))).collect();
        output(&summaries)?;
    } else if queue.is_empty() {
        println!("Nothing awaiting review.");
    } else {
        let w = id_width(queue.iter().map(|t| t.id.as_str()));
        for t in &queue {
            let who = if t.assignee.is_empty() {
                String::new()
            } else {
                format!(" ({})", t.assignee)
            };
            println!(
                "{} {} — {}{} [{}]",
                id_cell(&t.id, w),
                format_priority(&t.priority, eff.get(&t.id)),
                t.title,
                who,
                color_tags(&t.tags)
            );
        }
    }
    Ok(())
}

/// List in-progress tasks not updated for `hours` hours (`bea stale`).
pub fn cmd_stale(tasks: &HashMap<String, Task>, hours: u32, json: bool) -> Result<()> {
    let older_than = jiff::SignedDuration::from_hours(i64::from(hours));
    let stale = service::list_stale(tasks, Timestamp::now(), older_than);
    let eff = service::effective_priorities(tasks);

    if json {
        let summaries: Vec<_> = stale
            .iter()
            .map(|s| s.summary(eff.get(&s.task.id)))
            .collect();
        output(&summaries)?;
    } else if stale.is_empty() {
        println!("No in-progress tasks idle for {hours}h or more.");
    } else {
        let w = id_width(stale.iter().map(|s| s.task.id.as_str()));
        for s in &stale {
            let t = &s.task;
            let who = if t.assignee.is_empty() {
                String::new()
            } else {
                format!(" ({})", t.assignee)
            };
            println!(
                "{} {} — {}{} idle {}h [{}]",
                id_cell(&t.id, w),
                format_priority(&t.priority, eff.get(&t.id)),
                t.title,
                who,
                s.idle_hours(),
                color_tags(&t.tags)
            );
        }
    }
    Ok(())
}

/// The workflow reminder at the end of `bea prime`.
const PRIME_WORKFLOW: &str = "\
Workflow: `bea start <id> -a <name>` claims a ready task; `bea review <id>` submits finished work
(`bea done <id>` closes it outright); `bea release <id>` hands a claim back; `bea reject <id>` sends
reviewed work back for changes. `bea show <id>` has the details. A high attempt count means earlier
workers failed: try another approach.";

/// Print the session-start context for a coding agent (`bea prime`): the
/// first `limit` ready tasks, the tasks in progress, the review queue and a
/// reminder of the workflow verbs.
pub fn cmd_prime(tasks: &HashMap<String, Task>, limit: usize, json: bool) -> Result<()> {
    let prime = service::prime(tasks, limit);
    let eff = service::effective_priorities(tasks);

    if json {
        return output(&prime.summary(&eff));
    }

    // One ID width for all three sections, so they line up with each other.
    let w = id_width(
        prime
            .ready
            .iter()
            .chain(&prime.in_progress)
            .chain(&prime.review)
            .map(|t| t.id.as_str()),
    );
    let line = |t: &Task, extra: &[String]| {
        let extra: Vec<&str> = extra
            .iter()
            .map(String::as_str)
            .filter(|s| !s.is_empty())
            .collect();
        let extra = if extra.is_empty() {
            String::new()
        } else {
            format!(" ({})", extra.join(", "))
        };
        let tags = if t.tags.is_empty() {
            String::new()
        } else {
            format!(" [{}]", color_tags(&t.tags))
        };
        println!(
            "  {} {} — {}{}{}",
            id_cell(&t.id, w),
            format_priority(&t.priority, eff.get(&t.id)),
            t.title,
            extra,
            tags
        );
    };
    // An in-progress task is on its Nth attempt; on a ready or reviewed task
    // the count is of attempts already made.
    let attempt = |t: &Task| match t.attempt_count() {
        0 => String::new(),
        n => format!("attempt {n}"),
    };
    let earlier = |t: &Task| match t.attempt_count() {
        0 => String::new(),
        1 => "1 earlier attempt".to_string(),
        n => format!("{n} earlier attempts"),
    };

    println!("# Bears task context");
    println!();
    if prime.ready_total == 0 {
        println!("Ready: none.");
    } else {
        match prime.ready_more() {
            0 => println!("Ready ({}):", prime.ready_total),
            n => println!(
                "Ready ({} of {}, {n} not shown; `bea ready` lists all):",
                prime.ready.len(),
                prime.ready_total
            ),
        }
        for t in &prime.ready {
            line(t, &[earlier(t)]);
        }
    }
    println!();
    if prime.in_progress.is_empty() {
        println!("In progress: none.");
    } else {
        println!("In progress ({}):", prime.in_progress.len());
        for t in &prime.in_progress {
            let who = if t.assignee.is_empty() {
                "unassigned".to_string()
            } else {
                t.assignee.clone()
            };
            line(t, &[who, attempt(t)]);
        }
    }
    println!();
    if prime.review.is_empty() {
        println!("Awaiting review: none.");
    } else {
        println!("Awaiting review ({}):", prime.review.len());
        for t in &prime.review {
            line(t, &[t.assignee.clone(), earlier(t)]);
        }
    }
    println!();
    println!("{PRIME_WORKFLOW}");
    Ok(())
}

/// Submit an in-progress task for review (`bea review <id>`).
pub fn cmd_review(base: &Path, tasks: &HashMap<String, Task>, id: &str, json: bool) -> Result<()> {
    let t = service::review_task(base, tasks, id)?;
    print_transition(&t, "awaiting review", json)
}

pub fn cmd_reject(base: &Path, tasks: &HashMap<String, Task>, id: &str, json: bool) -> Result<()> {
    let t = service::reject_task(base, tasks, id)?;
    print_transition(&t, "changes requested", json)
}

pub fn cmd_propose(base: &Path, tasks: &HashMap<String, Task>, id: &str, json: bool) -> Result<()> {
    let t = service::propose_task(base, tasks, id)?;
    print_transition(&t, "awaiting acceptance", json)
}

pub fn cmd_accept(base: &Path, tasks: &HashMap<String, Task>, id: &str, json: bool) -> Result<()> {
    let t = service::accept_task(base, tasks, id)?;
    print_transition(&t, "accepted", json)
}

/// Report a workflow move as `[id] title → status (note)`.
fn print_transition(t: &Task, note: &str, json: bool) -> Result<()> {
    if json {
        output(&t.summary(None))?;
    } else {
        println!(
            "[{}] {} → {} ({})",
            color_id(&t.id),
            t.title,
            color_status(&t.status),
            note
        );
    }
    Ok(())
}

pub fn cmd_epics(tasks: &HashMap<String, Task>, json: bool) -> Result<()> {
    let mut epics: Vec<&Task> = tasks.values().filter(|t| t.task_type.is_epic()).collect();
    epics.sort_by(|a, b| task::canonical_order(a, b));

    if json {
        let out: Vec<_> = epics
            .iter()
            .map(|t| t.epic_summary(service::epic_progress(tasks, &t.id)))
            .collect();
        output(&out)?;
    } else if epics.is_empty() {
        println!("No epics found.");
    } else {
        let w = id_width(epics.iter().map(|t| t.id.as_str()));
        for t in &epics {
            let p = service::epic_progress(tasks, &t.id);
            let prefix = "Epic:"
                .if_supports_color(Stdout, |t| t.bright_magenta())
                .to_string();
            println!(
                "{} {} {} {} {} [{}/{}] [{}]",
                id_cell(&t.id, w),
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

/// Display an archived task (no active task context — effective priority not available).
/// `plan` is ignored for archived tasks (archive is read-only; children may not be present).
pub fn cmd_show_archived(task: &Task, _plan: bool, json: bool) -> Result<()> {
    if json {
        let detail = task.detail(None);
        // Annotate JSON output so callers know this came from the archive
        let mut json_val = serde_json::to_value(&detail)?;
        if let Some(obj) = json_val.as_object_mut() {
            obj.insert("archived".to_string(), serde_json::Value::Bool(true));
        }
        output(&json_val)?;
    } else {
        println!(
            "[{}] {} {}",
            color_id(&task.id),
            task.title.if_supports_color(Stdout, |t| t.bold()),
            "(archived)".if_supports_color(Stdout, |t| t.dimmed()),
        );
        println!(
            "{} {}",
            "Status:  ".if_supports_color(Stdout, |t| t.bold()),
            color_status(&task.status)
        );
        println!(
            "{} {}",
            "Priority:".if_supports_color(Stdout, |t| t.bold()),
            color_priority(&task.priority)
        );
        println!(
            "{} {}",
            "Tags:    ".if_supports_color(Stdout, |t| t.bold()),
            if task.tags.is_empty() {
                "—".to_string()
            } else {
                color_tags(&task.tags)
            }
        );
        if !task.depends_on.is_empty() {
            println!(
                "{} {}",
                "Deps:    ".if_supports_color(Stdout, |t| t.bold()),
                task.depends_on
                    .iter()
                    .map(|d| color_id(d))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        if let Some(parent) = task.parent() {
            println!(
                "{} {}",
                "Parent:  ".if_supports_color(Stdout, |t| t.bold()),
                color_id(parent)
            );
        }
        if !task.assignee.is_empty() {
            println!(
                "{} {}",
                "Assignee:".if_supports_color(Stdout, |t| t.bold()),
                task.assignee
            );
        }
        if task.attempt_count() > 0 {
            println!(
                "{} {}",
                "Attempts:".if_supports_color(Stdout, |t| t.bold()),
                task.attempt_count()
            );
        }
        println!(
            "{} {}",
            "Created: ".if_supports_color(Stdout, |t| t.bold()),
            task.created
        );
        println!(
            "{} {}",
            "Updated: ".if_supports_color(Stdout, |t| t.bold()),
            task.updated
        );
        if !task.body.is_empty() {
            println!("\n{}", task.body);
        }
    }
    Ok(())
}

pub fn cmd_show(tasks: &HashMap<String, Task>, id: &str, plan: bool, json: bool) -> Result<()> {
    if plan {
        return cmd_show_plan(tasks, id, json);
    }

    let t = service::get_task(tasks, id)?;
    let eff = service::effective_priorities(tasks);
    let ep = eff.get(&t.id);

    if json {
        output(&t.detail(ep))?;
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
        if let Some(parent) = t.parent() {
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
        if t.attempt_count() > 0 {
            println!(
                "{} {}",
                "Attempts:".if_supports_color(Stdout, |t| t.bold()),
                t.attempt_count()
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

fn cmd_show_plan(tasks: &HashMap<String, Task>, id: &str, json: bool) -> Result<()> {
    let plan = service::plan_epic(tasks, id)?;

    if !plan.cyclic.is_empty() {
        let ids: Vec<&str> = plan.cyclic.iter().map(|t| t.id.as_str()).collect();
        eprintln!(
            "warning: {} task(s) in a dependency cycle, appended unordered: {}",
            ids.len(),
            ids.join(", ")
        );
    }
    let all: Vec<&Task> = plan
        .tasks
        .iter()
        .chain(plan.cyclic.iter())
        .copied()
        .collect();

    if json {
        let eff = service::effective_priorities(tasks);
        let details: Vec<_> = all.iter().map(|t| t.detail(eff.get(&t.id))).collect();
        output(&details)?;
    } else {
        for (i, t) in all.iter().enumerate() {
            print!("# {}. {}\n\n", i + 1, t.title);
            if !t.body.is_empty() {
                print!("{}\n\n", t.body);
            }
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
    parent: Option<String>,
    json: bool,
) -> Result<()> {
    // Map CLI parent arg: None = unchanged, "" = root, "id" = under that epic.
    // A new parent moves the task, renaming it and everything below it.
    let parent_update: Option<Option<String>> =
        parent.map(|p| if p.is_empty() { None } else { Some(p) });
    let service::UpdatedTask { task: t, renamed } = service::update_task(
        base,
        tasks,
        id,
        status,
        priority,
        tags,
        assignee,
        body,
        title,
        parent_update,
    )?;

    if json {
        // The task summary, plus the old → new ID map when the task moved.
        let mut value = serde_json::to_value(t.summary(None))?;
        if !renamed.is_empty()
            && let Some(obj) = value.as_object_mut()
        {
            obj.insert("renamed".into(), serde_json::to_value(&renamed)?);
        }
        output(&value)?;
    } else {
        println!("Updated task {} — {}", t.id, t.title);
        for (old, new) in &renamed {
            println!("  renamed {} → {}", old, color_id(new));
        }
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
        output(&t.summary(None))?;
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

pub fn cmd_start(
    base: &Path,
    tasks: &HashMap<String, Task>,
    id: &str,
    assignee: Option<String>,
    json: bool,
) -> Result<()> {
    let t = service::start_task(base, tasks, id, assignee)?;

    if json {
        output(&t.summary(None))?;
    } else {
        let mut notes: Vec<String> = Vec::new();
        if !t.assignee.is_empty() {
            notes.push(t.assignee.clone());
        }
        if t.attempt_count() > 1 {
            notes.push(format!("attempt {}", t.attempt_count()));
        }
        let suffix = if notes.is_empty() {
            String::new()
        } else {
            format!(" ({})", notes.join(", "))
        };
        println!(
            "[{}] {} → {}{}",
            color_id(&t.id),
            t.title,
            color_status(&t.status),
            suffix
        );
    }
    Ok(())
}

pub fn cmd_release(base: &Path, tasks: &HashMap<String, Task>, id: &str, json: bool) -> Result<()> {
    let t = service::release_task(base, tasks, id)?;

    if json {
        output(&t.summary(None))?;
    } else {
        println!(
            "[{}] {} → {} (unassigned)",
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
    let resolved_dep = store::resolve_prefix(tasks, depends_on)?;
    let t = service::add_dependency(base, tasks, id, depends_on)?;

    if json {
        output(&t.summary(None))?;
    } else {
        println!("[{}] now depends on [{}]", t.id, resolved_dep);
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
    let resolved_dep = store::resolve_prefix(tasks, depends_on)?;
    let t = service::remove_dependency(base, tasks, id, depends_on)?;

    if json {
        output(&t.summary(None))?;
    } else {
        println!("[{}] no longer depends on [{}]", t.id, resolved_dep);
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
        output(&json_tree)?;
    } else {
        let eff = graph.effective_priorities_all(tasks);
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
    } else if node.seen {
        let style = Style::new().dimmed();
        format!(
            " {}",
            "(see above)".if_supports_color(Stdout, |s| s.style(style))
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
        // Use bounded adjacency list: exclude isolated nodes; `--all` controls done/cancelled.
        let adj = graph.bounded_adjacency_list(tasks, all, None, None);
        output(&adj)?;
        return Ok(());
    }

    if tasks.is_empty() {
        println!("No tasks.");
        return Ok(());
    }

    // Compute effective priorities once in O(V+E)
    let eff = graph.effective_priorities_all(tasks);

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
            .then_with(|| a.id.cmp(&b.id))
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
    let service::PrunedTasks {
        tasks: deleted,
        renamed,
    } = service::prune_tasks(base, tasks, include_done)?;
    let summaries: Vec<_> = deleted.iter().map(|t| t.summary(None)).collect();

    if json {
        // Always the same shape: `renamed` is empty when nothing was detached.
        output(&serde_json::json!({ "pruned": summaries, "renamed": renamed }))?;
    } else {
        if deleted.is_empty() {
            println!("No tasks to prune.");
        } else {
            for s in &summaries {
                println!("Pruned {} — {}", s.id, s.title);
            }
        }
        for (old, new) in &renamed {
            println!("  renamed {} → {}", old, color_id(new));
        }
    }
    Ok(())
}

pub fn cmd_delete(base: &Path, tasks: &HashMap<String, Task>, id: &str, json: bool) -> Result<()> {
    let service::DeletedTask { task: t, renamed } = service::delete_task(base, tasks, id)?;

    if json {
        // The task summary, plus the old → new ID map when children were
        // detached.
        let mut value = serde_json::to_value(t.summary(None))?;
        if !renamed.is_empty()
            && let Some(obj) = value.as_object_mut()
        {
            obj.insert("renamed".into(), serde_json::to_value(&renamed)?);
        }
        output(&value)?;
    } else {
        println!("Deleted task {} — {}", t.id, t.title);
        for (old, new) in &renamed {
            println!("  renamed {} → {}", old, color_id(new));
        }
    }
    Ok(())
}

pub fn cmd_search(tasks: &HashMap<String, Task>, query: &str, all: bool, json: bool) -> Result<()> {
    let results = service::search_tasks(tasks, query, all);

    if json {
        let summaries: Vec<_> = results.iter().map(|t| t.summary(None)).collect();
        output(&summaries)?;
    } else {
        if results.is_empty() {
            println!("No tasks matching \"{query}\".");
        } else {
            let w = id_width(results.iter().map(|t| t.id.as_str()));
            for t in &results {
                println!(
                    "{} {} {} — {} [{}]",
                    id_cell(&t.id, w),
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

/// `bea archive [id]` — archive a single task (cascade) or sweep all archivable tasks.
pub async fn cmd_archive(
    base: &Path,
    tasks: &HashMap<String, Task>,
    id: Option<&str>,
    json: bool,
) -> Result<()> {
    let archived_ids: Vec<String> = match id {
        Some(id_or_prefix) => service::archive_task(base, tasks, id_or_prefix)?,
        None => service::archive_all(base, tasks)?,
    };

    if json {
        output(&serde_json::json!({ "archived": archived_ids }))?;
    } else if archived_ids.is_empty() {
        println!("Nothing to archive.");
    } else {
        for id in &archived_ids {
            println!("Archived {id}");
        }
    }
    Ok(())
}

/// `bea restore <id>` — restore a task (and its cascade) from the archive.
pub async fn cmd_restore(base: &Path, id: &str, json: bool) -> Result<()> {
    let restored_ids = service::restore_task(base, id).await?;

    if json {
        output(&serde_json::json!({ "restored": restored_ids }))?;
    } else {
        for id in &restored_ids {
            println!("Restored {id}");
        }
    }
    Ok(())
}

/// `bea list --archived` — list tasks in the archive.
pub async fn cmd_list_archived(base: &Path, json: bool) -> Result<()> {
    let archived = service::list_archive(base, None).await?;

    if json {
        let summaries: Vec<_> = archived.iter().map(|t| t.summary(None)).collect();
        output(&summaries)?;
    } else if archived.is_empty() {
        println!("No archived tasks.");
    } else {
        let w = id_width(archived.iter().map(|t| t.id.as_str()));
        for t in &archived {
            println!(
                "{} {} {} — {} [{}]",
                id_cell(&t.id, w),
                color_priority(&t.priority),
                color_status(&t.status),
                t.title,
                color_tags(&t.tags)
            );
        }
    }
    Ok(())
}

/// `bea log [--limit N]` — display archived tasks as a chronological log (most-recent-first).
pub async fn cmd_log(base: &Path, limit: Option<usize>, json: bool) -> Result<()> {
    let archived = service::list_archive(base, limit).await?;

    if json {
        let summaries: Vec<_> = archived.iter().map(|t| t.summary(None)).collect();
        output(&summaries)?;
    } else if archived.is_empty() {
        println!("No archived tasks.");
    } else {
        let w = id_width(archived.iter().map(|t| t.id.as_str()));
        for t in &archived {
            println!(
                "{} {} {} — {} [{}]",
                id_cell(&t.id, w),
                color_priority(&t.priority),
                color_status(&t.status),
                t.title,
                color_tags(&t.tags)
            );
        }
    }
    Ok(())
}

/// `bea migrate [--dry-run]` — convert a store written before hierarchical IDs.
pub fn cmd_migrate(base: &Path, dry_run: bool, json: bool) -> Result<()> {
    let migration = store::migrate(base, dry_run)?;

    if json {
        let orphaned: Vec<_> = migration
            .orphaned
            .iter()
            .map(|o| {
                serde_json::json!({
                    "id": o.id,
                    "parent": o.parent,
                    "reason": o.reason.to_string(),
                })
            })
            .collect();
        output(&serde_json::json!({
            "dry_run": dry_run,
            "renamed": migration.renamed,
            "orphaned": orphaned,
            "legacy_files": migration.legacy_files,
        }))?;
        return Ok(());
    }

    if migration.is_noop() {
        println!("Nothing to migrate: the store already uses hierarchical IDs.");
        return Ok(());
    }
    for (old, new) in &migration.renamed {
        println!("  renamed {} → {}", old, color_id(new));
    }
    for o in &migration.orphaned {
        let why = match o.reason {
            store::OrphanReason::MissingParent => "does not exist",
            store::OrphanReason::ParentNotEpic => "is not an epic",
            store::OrphanReason::ParentCycle => "is part of a parent cycle",
        };
        println!(
            "  kept {} as is: its parent {} {}, so the link was dropped",
            color_id(&o.id),
            o.parent,
            why
        );
    }
    let renamed = migration.renamed.len();
    if dry_run {
        println!("Dry run: would rename {renamed} task(s); nothing was written.");
    } else {
        println!(
            "Migrated the store: renamed {renamed} task(s), {} file(s) had a parent: key.",
            migration.legacy_files
        );
    }
    Ok(())
}

pub fn cmd_edit(base: &Path, tasks: &HashMap<String, Task>, id: &str, json: bool) -> Result<()> {
    // Resolve prefix and load task
    let original = service::get_task(tasks, id)?;
    let original_id = original.id.clone();
    let path = store::find_task_path(base, &original_id)?;

    crate::editor::open_in_editor(&path)?;

    // Re-read and validate; an unparseable result is an error (non-zero exit)
    let content = std::fs::read_to_string(&path).map_err(Error::Io)?;
    let mut edited = match task::parse_task(&content) {
        Ok(t) => t,
        Err(e) => {
            let reason = match e {
                Error::InvalidFrontmatter { reason, .. } => reason,
                other => other.to_string(),
            };
            return Err(Error::InvalidFrontmatter {
                path: path.clone(),
                reason: format!("{reason} (file left on disk — fix and retry)"),
            });
        }
    };

    // The ID is the task's identity — changing it in the editor would orphan
    // the old file and duplicate the task, so force it back.
    let id_changed = edited.id != original_id;
    if id_changed {
        eprintln!("warning: task ID cannot be changed via edit; keeping {original_id}");
        edited.id = original_id.clone();
    }

    // Check if anything (besides a reverted ID) changed
    if !id_changed && task::render_task(&original) == task::render_task(&edited) {
        if !json {
            println!("No changes.");
        }
        return Ok(());
    }

    // Save with updated timestamp (and handle slug rename if title changed)
    edited.updated = Timestamp::now();
    store::save(base, &edited)?;

    if json {
        output(&edited.summary(None))?;
    } else {
        println!("Edited task {} — {}", edited.id, edited.title);
    }
    Ok(())
}

/// `bea snapshot [--format html|md] [-o FILE] [--all]` — render the store as
/// a static artifact, to stdout or to `output`.
///
/// The time in the header is the only thing taken from outside the store.
/// With `output` nothing is printed, so `bea snapshot > f` and
/// `bea snapshot -o f` write the same bytes. The store is loaded before the
/// file is opened, so a refused load (not initialized, legacy store) leaves
/// no file behind. A path inside `.bears/` is refused (see
/// [`refuse_output_in_store`]) and the file is written atomically (see
/// [`write_atomically`]).
pub async fn cmd_snapshot(
    base: &Path,
    format: SnapshotFormat,
    output: Option<&Path>,
    all: bool,
    json: bool,
) -> Result<()> {
    if json {
        return Err(Error::InvalidUsage(
            "`bea snapshot` has no JSON output; use --format md or --format html \
             (the other commands' --json output covers the data)"
                .to_string(),
        ));
    }

    let options = snapshot::SnapshotOptions {
        include_all: all,
        generated_at: Some(Timestamp::now()),
        ..Default::default()
    };
    let snap = snapshot::load(base, &options).await?;
    let rendered = match format {
        SnapshotFormat::Md => snapshot::markdown::render(&snap),
        SnapshotFormat::Html => snapshot::html::render(&snap),
    };

    match output {
        Some(path) => {
            refuse_output_in_store(base, path)?;
            write_atomically(path, rendered.as_bytes()).map_err(|e| {
                Error::Io(std::io::Error::new(
                    e.kind(),
                    format!("cannot write {}: {e}", path.display()),
                ))
            })?
        }
        None => print!("{rendered}"),
    }
    Ok(())
}

/// Refuse an output path that resolves to `.bears/` or anything below it:
/// every loading command would then warn about the file, and `bea doctor`
/// report it, until it was deleted.
///
/// The location is resolved, not the spelling, so `./.bears/x`, an absolute
/// path, `..` and symlinked directories are all seen through, while a name
/// that merely contains `.bears` elsewhere is fine. The file usually does not
/// exist yet, so its parent directory is resolved and the file name appended.
/// A parent that cannot be resolved (it does not exist) is left to the write,
/// which fails naming the path. An existing symlink at the target itself is
/// not followed, because the write replaces it rather than writing through it.
fn refuse_output_in_store(base: &Path, path: &Path) -> Result<()> {
    let Ok(store_dir) = store::tasks_dir(base).canonicalize() else {
        return Ok(());
    };
    let resolved = match path.file_name() {
        Some(name) => {
            let parent = match path.parent() {
                Some(p) if !p.as_os_str().is_empty() => p,
                _ => Path::new("."),
            };
            parent.canonicalize().map(|p| p.join(name))
        }
        // `..` or `/`: no file name to append, resolve the whole path.
        None => path.canonicalize(),
    };
    match resolved {
        Ok(resolved) if resolved.starts_with(&store_dir) => Err(Error::InvalidUsage(format!(
            "refusing to write the snapshot to {}: it is inside the .bears/ store, \
             where every command would try to read it as a task file; write it \
             outside .bears/",
            path.display()
        ))),
        _ => Ok(()),
    }
}

/// Write `contents` to `path` atomically: into a fresh temporary file in the
/// same directory, then renamed over `path`, so a job killed mid-write leaves
/// either the old file or the new one, never a truncated one.
///
/// The temporary name carries the process ID, the time and a counter, and is
/// created with `create_new`, so a concurrent `bea snapshot -o` to the same
/// target never shares it. It is removed when writing or renaming fails. An
/// existing regular file's permissions are carried over; an existing symlink
/// is replaced by the new file, not followed.
fn write_atomically(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    use std::fs::{self, OpenOptions};
    use std::io::{Error as IoError, ErrorKind, Write};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    let name = path
        .file_name()
        .ok_or_else(|| IoError::new(ErrorKind::InvalidInput, "not a file name"))?;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or_default();

    let mut attempts = 0;
    let (tmp_path, mut file) = loop {
        attempts += 1;
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let tmp_name = format!(
            ".{}.{}-{nanos}-{n}.tmp",
            name.to_string_lossy(),
            std::process::id()
        );
        let tmp_path = path.with_file_name(tmp_name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)
        {
            Ok(file) => break (tmp_path, file),
            Err(e) if e.kind() == ErrorKind::AlreadyExists && attempts < 100 => continue,
            Err(e) => return Err(e),
        }
    };

    let result = (|| {
        file.write_all(contents)?;
        if let Ok(meta) = fs::symlink_metadata(path)
            && meta.is_file()
        {
            file.set_permissions(meta.permissions())?;
        }
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp_path, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp_path);
    }
    result
}

/// `bea doctor [--fix]` — check the store for inconsistencies, and with
/// `--fix` repair duplicate IDs and misnamed files first.
///
/// Prints the report and exits with status 1 when problems are found (after
/// `--fix`, when problems remain), so it can gate CI.
pub fn cmd_doctor(base: &Path, fix: bool, json: bool) -> Result<()> {
    let report = if fix {
        service::repair_store(base)?
    } else {
        service::check_store(base)?
    };

    if json {
        let mut value = serde_json::to_value(&report)?;
        value["ok"] = report.is_healthy().into();
        value["needs_review"] = report.needs_review().into();
        if fix {
            for key in ["renamed", "moved"] {
                if value.get(key).is_none() {
                    value[key] = serde_json::json!([]);
                }
            }
        }
        output(&value)?;
    } else {
        for r in &report.renamed {
            println!(
                "  renamed {} → {} ({}, {:?})",
                r.from,
                color_id(&r.to),
                r.file.display(),
                r.title
            );
            println!(
                "    {} stays with {} ({:?})",
                r.from,
                r.kept_file.display(),
                r.kept_title
            );
            if let Some(attached) = r.describe_attached() {
                println!(
                    "    check these still belong to that copy, not to {}: {attached}",
                    r.to
                );
                println!(
                    "    (move a task with `bea update <id> --parent {}`, a dependency with `bea dep`)",
                    r.to
                );
            }
        }
        for m in &report.moved {
            println!("  moved {} → {}", m.from.display(), m.to.display());
        }
        let to_check = report.renamed.iter().filter(|r| r.has_attached()).count();
        if report.is_healthy() && report.needs_review() {
            println!(
                "Checked {} task file(s): no problems remain, but tasks referring to \
                 {to_check} renumbered ID(s) were left with the copy that kept the ID; review \
                 the tasks listed above.",
                report.files
            );
        } else if report.is_healthy() {
            println!("Checked {} task file(s): no problems found.", report.files);
        } else {
            println!(
                "Found {} problem(s) in {} task file(s):",
                report.findings.len(),
                report.files
            );
            for f in &report.findings {
                println!("  {f}");
            }
            if report
                .findings
                .iter()
                .any(|f| matches!(f, service::Finding::LegacyStore { .. }))
            {
                eprintln!(
                    "hint: run `bea migrate` to give child tasks hierarchical IDs, then `bea doctor` again"
                );
            } else {
                let fixable = report.findings.iter().filter(|f| f.is_fixable()).count();
                if !fix && fixable > 0 {
                    eprintln!("hint: `bea doctor --fix` repairs {fixable} of them");
                }
            }
        }
    }

    if !report.is_healthy() {
        use std::io::Write;
        std::io::stdout().flush()?;
        std::process::exit(1);
    }
    Ok(())
}
