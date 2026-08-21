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
        _ => {
            restore_sigpipe();
            cli::run(args, base).await
        }
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        if let Some(hint) = hint_for(&e) {
            eprintln!("hint: {hint}");
        }
        std::process::exit(1);
    }
}

/// Restore the default `SIGPIPE` disposition before writing CLI output.
///
/// Rust ignores `SIGPIPE` at startup, so writing to a closed pipe returns
/// `EPIPE` and `println!` turns that into a panic — `bea show | head` would
/// print a panic message after `head` exits. Unix filters are expected to die
/// from the signal instead, quietly and with the conventional 141 exit status.
///
/// Only the CLI does this. The MCP server and TUI own their transports and
/// handle shutdown themselves, so they keep Rust's default behaviour.
#[cfg(unix)]
fn restore_sigpipe() {
    // SAFETY: resetting a signal to `SIG_DFL` changes process-wide disposition
    // and installs no handler, so there is no Rust code to run from a signal
    // context. Nothing else in the process depends on `SIGPIPE` being ignored.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

/// Windows has no `SIGPIPE`; a closed pipe surfaces as an ordinary write error.
#[cfg(not(unix))]
fn restore_sigpipe() {}

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
