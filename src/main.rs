mod cli;
mod editor;
mod mcp;
mod tui;

use crate::cli::Command;
use bears::error::Error;
use clap::Parser;
use cli::Args;
use std::path::Path;

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let base = Path::new(".");

    let result = match args.command {
        Command::Mcp => mcp::run(base).await,
        Command::Tui => tui::run(base).await,
        _ => cli::run(args, base).await,
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        if let Some(hint) = hint_for(&e) {
            eprintln!("hint: {hint}");
        }
        std::process::exit(1);
    }
}

/// CLI-specific remediation hints.
///
/// The library deliberately keeps frontend suggestions out of its error
/// messages, so the `bea` binary attaches them here.
fn hint_for(e: &Error) -> Option<&'static str> {
    match e {
        Error::NotInitialized => Some("run `bea init` to create a .bears/ directory"),
        Error::NotArchived(_) => Some("use `bea log` to list archived tasks"),
        Error::InvalidStatus { .. } => {
            Some("`bea status <id> <status>` sets any status directly if you need a different move")
        }
        Error::NotArchivable { .. } => {
            Some("archive or complete the blocking dependents first, or use `bea archive` to sweep")
        }
        _ => None,
    }
}
