# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

`bea-rs` is a file-based task tracker CLI tool named `bears` (binary: `bea`). It manages a task/issue graph stored as markdown files with YAML frontmatter in a `.bears/` directory. It has two modes:

1. **CLI mode** (`bea <command>`): Human-friendly interface for managing tasks
2. **MCP server mode** (`bea mcp`): Exposes the same functionality as MCP tool calls over stdio for AI agents

## Commands

```bash
cargo build                  # Build debug binary
cargo build --release        # Build release binary
cargo run -- <args>          # Run with arguments
cargo test                   # Run all tests
cargo test <test_name>       # Run a specific test
cargo clippy                 # Lint
cargo fmt                    # Format code
cargo add <crate>            # Add a new dependency (always use this, not manual Cargo.toml edits)
```

## Task tracking

This project uses `bears` itself to manage its own tasks. The bears MCP server is registered with Claude Code — use the MCP tools (`list_ready`, `create_task`, `complete_task`, etc.) to check and update tasks rather than running `bea` CLI commands directly.

## Development workflow

Work test-driven: write tests before or alongside implementation. After finishing any task, always run:

```bash
cargo fmt && cargo clippy && cargo test
```

All three must pass cleanly before considering a task done.

## Commit style

Use [Conventional Commits](https://www.conventionalcommits.org/): `type(scope): description`. Common types: `feat`, `fix`, `ci`, `docs`, `chore`, `refactor`, `test`. Release notes are generated from commit messages, so keep them accurate and descriptive.

## Architecture

```
src/
  main.rs          # Entry point: dispatch to CLI or MCP server
  cli/
    mod.rs         # CLI module root and dispatch
    args.rs        # clap command and argument definitions
    cmd.rs         # Command handlers (list, show, create, etc.)
  mcp/
    mod.rs         # MCP module root and server setup
    params.rs      # Tool parameter structs (serde + JSON Schema)
    tools.rs       # MCP tool implementations and tests
  service.rs       # Business logic: create, update, epic progress, auto-close
  store.rs         # Core: parse .bears/ dir, read/write task files
  task.rs          # Task struct, frontmatter serde, ID generation
  graph.rs         # Dependency graph: build, query ready, cycle detection
  config.rs        # .bears.yml configuration loading
  error.rs         # thiserror error types
```

### Core design principles

- `store.rs`, `service.rs`, and `graph.rs` are the core library. CLI and MCP are thin frontends.
- Re-parse the entire `.bears/` directory from scratch on every invocation — no caching, no daemon.
- `--json` flag on all CLI commands outputs JSON instead of human text.
- MCP tools return minimal structured data (id, title, priority, status, tags) — not full markdown bodies.

### Storage format

Tasks are stored as `.bears/{id}-{slug}.md` with YAML frontmatter:

```markdown
---
id: a1b2
title: Implement OAuth flow
status: open          # open | in_progress | done | blocked | cancelled
priority: P1          # P0 (critical) through P3
type: task            # task (default) | epic
created: 2026-03-15T10:30:00Z
updated: 2026-03-15T10:30:00Z
tags: [backend, auth]
depends_on: [f4c9, b7e3]
parent: x9k2
assignee: ""
---

Markdown body here.
```

Parse frontmatter by splitting on `---` delimiters, using `serde_yaml` for the YAML portion, keeping the rest as the body string.

### ID generation

Generate a short lowercase alphanumeric ID (configurable length via `.bears.yml`, default 3 chars). Check for collisions; regenerate if needed.

### `ready` command

The key command for agent workflows. Returns tasks where status is `open`, type is `task` (not `epic`), AND all `depends_on` tasks have status `done`. Sort by priority (P0 first), then creation date.

### Epic behavior

Epics group child tasks via the `parent` field. `bea epics` / `list_epics` shows epics with progress (done/total children). When all children of an epic are completed, the epic is automatically marked as done. Epics are excluded from `ready` results.

### Cycle detection

Validate on `dep add` that the new edge doesn't create a cycle. Reject with a clear error if it would.

### MCP tools

| Tool | Key params |
|------|------------|
| `list_ready` | `limit?`, `tag?`, `epic?` |
| `list_all_tasks` | `status?`, `priority?`, `tag?`, `epic?` |
| `list_epics` | — |
| `get_task` | `id` |
| `create_task` | `title`, `priority?`, `tags?`, `depends_on?`, `parent?`, `body?`, `type?` |
| `update_task` | `id`, `status?`, `priority?`, `tags?`, `assignee?`, `body?` |
| `start_task` | `id` |
| `complete_task` | `id` |
| `cancel_task` | `id` |
| `prune_tasks` | `include_done?` |
| `add_dependency` | `id`, `depends_on` |
| `remove_dependency` | `id`, `depends_on` |
| `delete_task` | `id` |
| `search_tasks` | `query` |
| `get_graph` | — |

## Dependencies

- `clap` (derive) — CLI parsing
- `clap_complete` — shell completion generation
- `serde`, `serde_yaml`, `serde_json` — serialization
- `chrono` — timestamps
- `rand` — ID generation
- `thiserror` — error types
- `tokio` — async runtime used for MCP server, parallel file loading in `store::load_all`
- `rmcp` — MCP SDK (server, macros, transport-io features)
- `schemars` — JSON Schema generation for MCP tool parameter types
- `owo-colors` — terminal color output

Keep the dependency tree small. Compilation should be fast.

## Error handling

- `.bears/` not found → suggest `bea init`
- Task ID not found → clear error
- Cycle detected → reject with explanation
- Invalid frontmatter → warn and skip (don't crash), report which file
- Library code uses `Result` with `?`. CLI formats errors for humans. MCP returns `rmcp::ErrorData` from tool methods.

## Testing

- Unit tests in `graph.rs`: cycle detection, topological sort, ready computation
- Unit tests in `task.rs`/`store.rs`: frontmatter parsing (valid, missing fields, extra fields, malformed)
- Unit tests in `service.rs`: epic progress, auto-close
- Unit tests in `mcp/tools.rs`: tool create/list/start/complete/search/graph/delete/deps/validation
- Integration tests in `tests/cli.rs`: create a temp `.bears/` dir, run commands, verify file output
- MCP tools also verified end-to-end via live MCP sessions
