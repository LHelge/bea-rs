mod args;
mod cmd;

pub use args::{Args, Command, DepCommand};

use std::path::Path;

use clap::CommandFactory;
use owo_colors::OwoColorize;
use owo_colors::Stream::Stdout;
use serde::Serialize;

use crate::error::{Error, Result};
use crate::service;
use crate::store;
use crate::task::{Priority, Status};

pub async fn run(cli: Args, base: &Path) -> Result<()> {
    // Handle commands that don't need task data
    match &cli.command {
        Command::Init {
            claude,
            copilot,
            codex,
        } => return cmd::cmd_init(base, *claude, *copilot, *codex, cli.json),
        Command::Completions { shell } => {
            clap_complete::generate(*shell, &mut Args::command(), "bea", &mut std::io::stdout());
            return Ok(());
        }
        // Archive commands that work directly on the archive (no active task load needed
        // for restore/list-archived/log).
        Command::Restore { id } => {
            return cmd::cmd_restore(base, id, cli.json).await;
        }
        Command::Log { limit } => {
            return cmd::cmd_log(base, *limit, cli.json).await;
        }
        Command::List { archived: true, .. } => {
            return cmd::cmd_list_archived(base, cli.json).await;
        }
        Command::Mcp | Command::Tui => unreachable!("handled in main"),
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
            epic,
        } => cmd::cmd_create(
            base, &tasks, title, priority, tag, depends_on, parent, body, epic, cli.json,
        ),
        Command::List {
            status,
            priority,
            tag,
            epic,
            all,
            archived: false,
        } => cmd::cmd_list(&tasks, status, priority, tag, epic, all, cli.json),
        Command::Ready { tag, epic, limit } => cmd::cmd_ready(&tasks, tag, epic, limit, cli.json),
        Command::Epics => cmd::cmd_epics(&tasks, cli.json),
        Command::Show { id, plan } => {
            // Try active tasks first; fall back to archive with a clear label.
            match cmd::cmd_show(&tasks, &id, plan, cli.json) {
                Err(Error::TaskNotFound(_)) => {
                    // Not in active tasks — try the archive
                    let archived_task = service::get_archived_task(base, &id).await;
                    match archived_task {
                        Ok(task) => {
                            if !cli.json {
                                eprintln!(
                                    "Note: task {} is archived. Use `bea restore {}` to make it active again.",
                                    task.id, task.id
                                );
                            }
                            cmd::cmd_show_archived(&task, plan, cli.json)
                        }
                        Err(_) => Err(Error::TaskNotFound(id)),
                    }
                }
                other => other,
            }
        }
        Command::Update {
            id,
            status,
            priority,
            tag,
            assignee,
            body,
            title,
            parent,
        } => {
            let result = cmd::cmd_update(
                base, &tasks, &id, status, priority, tag, assignee, body, title, parent, cli.json,
            );
            augment_archived_error(base, result, &id).await
        }
        Command::Status { id, status } => {
            let result = cmd::cmd_status(base, &tasks, &id, status, cli.json);
            augment_archived_error(base, result, &id).await
        }
        Command::Start { id } => {
            let result = cmd::cmd_status(base, &tasks, &id, Status::InProgress, cli.json);
            augment_archived_error(base, result, &id).await
        }
        Command::Done { id } => {
            let result = cmd::cmd_status(base, &tasks, &id, Status::Done, cli.json);
            augment_archived_error(base, result, &id).await
        }
        Command::Dep { command } => match command {
            DepCommand::Add { id, depends_on } => {
                cmd::cmd_dep_add(base, &tasks, &id, &depends_on, cli.json)
            }
            DepCommand::Remove { id, depends_on } => {
                cmd::cmd_dep_remove(base, &tasks, &id, &depends_on, cli.json)
            }
            DepCommand::Tree { id } => cmd::cmd_dep_tree(&tasks, &id, cli.json),
        },
        Command::Graph { all } => cmd::cmd_graph(&tasks, all, cli.json),
        Command::Cancel { id } => {
            let result = cmd::cmd_status(base, &tasks, &id, Status::Cancelled, cli.json);
            augment_archived_error(base, result, &id).await
        }
        Command::Prune { done } => cmd::cmd_prune(base, &tasks, done, cli.json),
        Command::Delete { id } => {
            let result = cmd::cmd_delete(base, &tasks, &id, cli.json);
            augment_archived_error(base, result, &id).await
        }
        Command::Search { query, all } => cmd::cmd_search(&tasks, &query, all, cli.json),
        Command::Edit { id } => {
            let result = cmd::cmd_edit(base, &tasks, &id, cli.json);
            augment_archived_error(base, result, &id).await
        }
        Command::Archive { id } => cmd::cmd_archive(base, &tasks, id.as_deref(), cli.json).await,
        // Already handled above
        Command::Init { .. }
        | Command::Completions { .. }
        | Command::Restore { .. }
        | Command::Log { .. }
        | Command::List { archived: true, .. }
        | Command::Mcp
        | Command::Tui => {
            unreachable!()
        }
    }
}

/// If `result` is `TaskNotFound` and the ID is in the archive, replace the error
/// with a friendlier message suggesting `bea restore`.  Otherwise returns the
/// original result unchanged.
async fn augment_archived_error<T>(base: &Path, result: Result<T>, id: &str) -> Result<T> {
    if matches!(result, Err(Error::TaskNotFound(_)))
        && service::get_archived_task(base, id).await.is_ok()
    {
        return Err(Error::TaskNotFound(format!(
            "{id} (task is archived — use `bea restore {id}` first)"
        )));
    }
    result
}

fn color_priority(p: &Priority) -> String {
    use owo_colors::Style;
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

fn output<T: Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
