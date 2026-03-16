mod cli;
mod error;
mod graph;
mod mcp;
mod service;
mod store;
mod task;

use crate::cli::Command;
use clap::Parser;
use cli::Args;
use std::path::Path;

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let base = Path::new(".");

    let result = if args.command == Command::Mcp {
        mcp::run(base).await
    } else {
        cli::run(args, base).await
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
