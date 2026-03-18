mod args;
mod cmd;

pub use args::{Args, Command, DepCommand};

use std::path::Path;

use clap::CommandFactory;
use owo_colors::OwoColorize;
use owo_colors::Stream::Stdout;
use serde::Serialize;

use crate::error::Result;
use crate::store;
use crate::task::{Priority, Status};

pub async fn run(cli: Args, base: &Path) -> Result<()> {
    // Handle commands that don't need task data
    match &cli.command {
        Command::Init => return cmd::cmd_init(base, cli.json),
        Command::Completions { shell } => {
            clap_complete::generate(*shell, &mut Args::command(), "bea", &mut std::io::stdout());
            return Ok(());
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
        } => cmd::cmd_list(&tasks, status, priority, tag, epic, all, cli.json),
        Command::Ready { tag, epic, limit } => cmd::cmd_ready(&tasks, tag, epic, limit, cli.json),
        Command::Epics => cmd::cmd_epics(&tasks, cli.json),
        Command::Show { id, plan } => cmd::cmd_show(&tasks, &id, plan, cli.json),
        Command::Update {
            id,
            status,
            priority,
            tag,
            assignee,
            body,
            title,
        } => cmd::cmd_update(
            base, &tasks, &id, status, priority, tag, assignee, body, title, cli.json,
        ),
        Command::Status { id, status } => cmd::cmd_status(base, &tasks, &id, status, cli.json),
        Command::Start { id } => cmd::cmd_status(base, &tasks, &id, Status::InProgress, cli.json),
        Command::Done { id } => cmd::cmd_status(base, &tasks, &id, Status::Done, cli.json),
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
        Command::Cancel { id } => cmd::cmd_status(base, &tasks, &id, Status::Cancelled, cli.json),
        Command::Prune { done } => cmd::cmd_prune(base, &tasks, done, cli.json),
        Command::Delete { id } => cmd::cmd_delete(base, &tasks, &id, cli.json),
        Command::Search { query, all } => cmd::cmd_search(&tasks, &query, all, cli.json),
        Command::Edit { id } => cmd::cmd_edit(base, &tasks, &id, cli.json),
        // Already handled above
        Command::Init | Command::Completions { .. } | Command::Mcp | Command::Tui => {
            unreachable!()
        }
    }
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

fn output<T: Serialize>(value: &T, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(value)?);
    }
    Ok(())
}
