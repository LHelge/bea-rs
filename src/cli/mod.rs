mod args;
mod cmd;

pub use args::{AgentCategory, Args, Command, DepCommand};

use std::path::Path;

use clap::CommandFactory;
use owo_colors::OwoColorize;
use owo_colors::Stream::Stdout;
use serde::Serialize;

use bears::error::{Error, Result};
use bears::service;
use bears::store;
use bears::task::{Priority, Status};

/// Resolve an `--epic` filter value (an ID or a prefix) to the epic's ID.
async fn resolve_epic(
    base: &Path,
    tasks: &std::collections::HashMap<String, bears::Task>,
    epic: Option<String>,
) -> Result<Option<String>> {
    match epic {
        Some(e) => Ok(Some(service::resolve_epic_filter(base, tasks, &e).await?)),
        None => Ok(None),
    }
}

pub async fn run(cli: Args, base: &Path) -> Result<()> {
    // Handle commands that don't need task data
    match &cli.command {
        Command::Init {
            claude,
            copilot,
            codex,
            force,
        } => return cmd::cmd_init(base, *claude, *copilot, *codex, *force, cli.json),
        Command::Agent {
            category,
            claude,
            copilot,
            codex,
            force,
            append,
        } => {
            return cmd::cmd_agent_init(
                base, *category, *claude, *copilot, *codex, *force, *append, cli.json,
            );
        }
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
        // Works on a legacy store, which every loader refuses.
        Command::Migrate { dry_run } => return cmd::cmd_migrate(base, *dry_run, cli.json),
        // Reads task files itself, so it can see what the loaders skip.
        Command::Doctor { fix } => return cmd::cmd_doctor(base, *fix, cli.json),
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
        } => {
            let epic = resolve_epic(base, &tasks, epic).await?;
            cmd::cmd_list(&tasks, status, priority, tag, epic, all, cli.json)
        }
        Command::Ready { tag, epic, limit } => {
            let epic = resolve_epic(base, &tasks, epic).await?;
            cmd::cmd_ready(&tasks, tag, epic, limit, cli.json)
        }
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
        Command::Start { id, assignee } => {
            let result = cmd::cmd_start(base, &tasks, &id, assignee, cli.json);
            augment_archived_error(base, result, &id).await
        }
        Command::Release { id } => {
            let result = cmd::cmd_release(base, &tasks, &id, cli.json);
            augment_archived_error(base, result, &id).await
        }
        Command::Review {
            id,
            tag,
            epic,
            limit,
        } => match id {
            Some(id) => {
                let result = cmd::cmd_review(base, &tasks, &id, cli.json);
                augment_archived_error(base, result, &id).await
            }
            None => {
                let epic = resolve_epic(base, &tasks, epic).await?;
                cmd::cmd_review_queue(&tasks, tag.as_deref(), epic.as_deref(), limit, cli.json)
            }
        },
        Command::Reject { id } => {
            let result = cmd::cmd_reject(base, &tasks, &id, cli.json);
            augment_archived_error(base, result, &id).await
        }
        Command::Propose { id } => {
            let result = cmd::cmd_propose(base, &tasks, &id, cli.json);
            augment_archived_error(base, result, &id).await
        }
        Command::Accept { id } => {
            let result = cmd::cmd_accept(base, &tasks, &id, cli.json);
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
        | Command::Agent { .. }
        | Command::Completions { .. }
        | Command::Restore { .. }
        | Command::Migrate { .. }
        | Command::Doctor { .. }
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
        Status::Proposed => s.if_supports_color(Stdout, |t| t.dimmed()).to_string(),
        Status::Open => s.to_string(),
        Status::InProgress => s.if_supports_color(Stdout, |t| t.cyan()).to_string(),
        Status::Review => s.if_supports_color(Stdout, |t| t.blue()).to_string(),
        Status::Done => s.if_supports_color(Stdout, |t| t.green()).to_string(),
        Status::Blocked => s.if_supports_color(Stdout, |t| t.red()).to_string(),
        Status::Cancelled => s.if_supports_color(Stdout, |t| t.dimmed()).to_string(),
    }
}

fn color_id(id: &str) -> String {
    id.if_supports_color(Stdout, |t| t.dimmed()).to_string()
}

/// Width of the longest of `ids`, for [`id_cell`]. Dotted IDs (`a1b.c2.d7`)
/// vary in length, so list output pads every ID to the longest one shown.
fn id_width<'a>(ids: impl IntoIterator<Item = &'a str>) -> usize {
    ids.into_iter()
        .map(|id| id.chars().count())
        .max()
        .unwrap_or(0)
}

/// `[id]` padded with trailing spaces to `width` ID characters, so the column
/// after it lines up for every row of a list.
fn id_cell(id: &str, width: usize) -> String {
    let pad = width.saturating_sub(id.chars().count());
    format!("[{}]{}", color_id(id), " ".repeat(pad))
}

fn color_tags(tags: &[String]) -> String {
    let joined = tags.join(", ");
    joined.if_supports_color(Stdout, |t| t.dimmed()).to_string()
}

fn output<T: Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_cells_pad_to_the_longest_id() {
        let ids = ["a1b", "a1b.c2", "a1b.c2.d7"];
        let width = id_width(ids);
        assert_eq!(width, 9);
        let cells: Vec<String> = ids.iter().map(|id| id_cell(id, width)).collect();
        assert_eq!(cells[0], "[a1b]      ");
        assert_eq!(cells[1], "[a1b.c2]   ");
        assert_eq!(cells[2], "[a1b.c2.d7]");
    }

    #[test]
    fn id_width_of_nothing_is_zero() {
        assert_eq!(id_width(std::iter::empty()), 0);
        assert_eq!(id_cell("a1b", 0), "[a1b]");
    }
}
