# bears 🐻

[![CI](https://github.com/LHelge/bea-rs/actions/workflows/push.yml/badge.svg)](https://github.com/LHelge/bea-rs/actions/workflows/push.yml)
[![crates.io](https://img.shields.io/crates/v/bea-rs)](https://crates.io/crates/bea-rs)
[![GitHub release](https://img.shields.io/github/v/release/LHelge/bea-rs)](https://github.com/LHelge/bea-rs/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A file-based task tracker for developers and AI agents.

Heavily inspired by Steve Yegge's [Beads](https://github.com/steveyegge/beads) that didn't really fit my workflow. 

Tasks live as Markdown files with YAML frontmatter in a `.bears/` directory — plain text, git-friendly, no database. Run `bea` from the terminal or expose the same functionality as an MCP server for AI coding agents.

---

## Install

### From crates.io

```sh
cargo install bea-rs
```

### Pre-built binary

Download the latest release for your platform:

```sh
curl -fsSL https://raw.githubusercontent.com/LHelge/bea-rs/main/install.sh | sh
```

This detects your OS and architecture, downloads the right binary from GitHub Releases, and installs it to `/usr/local/bin`. Set `BEA_INSTALL_DIR` to change the install location. Falls back to `cargo install` if no pre-built binary is available.

### From source

```sh
git clone https://github.com/LHelge/bea-rs.git
cd bea-rs
cargo install --path .
```

The binary is named `bea`.

---

## Quick start

```sh
bea init
bea create "Design the API" --priority P1 --tag backend
bea create "Implement endpoints" --priority P1 --tag backend --depends-on <id>
bea list
bea ready           # what can I work on right now?
bea start <id>
bea done <id>
```

---

## Task format

Each task is stored as `.bears/{id}-{slug}.md`:

```markdown
---
id: a1b2
title: Implement OAuth flow
status: open
priority: P1
type: task
created: 2026-03-15T10:30:00Z
updated: 2026-03-15T10:30:00Z
tags: [backend, auth]
depends_on: [f4c9]
parent: x9k2
---

Any Markdown body goes here.
```

**Statuses:** `open` · `in_progress` · `done` · `blocked` · `cancelled`

**Types:** `task` (default) · `epic` (high-level objective grouping child tasks)

**Priorities:** `P0` (critical) · `P1` · `P2` · `P3` (low) — sorted P0 first everywhere. A task inherits the highest priority of any task that depends on it, so a P3 task blocking a P0 task is effectively treated as P0.

---

## Commands

### `bea init`
Create the `.bears/` directory and `.bears.yml` config in the current directory.

```sh
bea init
```

### `bea create`
```sh
bea create "Title" [--priority P0-P3] [--tag tag1,tag2] [--depends-on id1,id2] [--body "..."] [--epic]
```

Use `--epic` to create an epic instead of a regular task. Epics are high-level objectives that group child tasks via the `--parent` flag.

### `bea list`
Hides `done` and `cancelled` tasks by default. Use `--all` / `-a` to show everything.

```sh
bea list
bea list --status open
bea list --priority P0
bea list --tag backend
bea list --epic <epic-id>
bea list --all
```

### `bea ready`
Show tasks that are `open` and have all dependencies completed. Epics are excluded — only actionable tasks appear. This is the key command for agent workflows — always start here.

```sh
bea ready
bea ready --tag backend --limit 5
bea ready --epic <epic-id>
```

### `bea show`
```sh
bea show <id>
```

### `bea update`
```sh
bea update <id> --status blocked
bea update <id> --priority P0 --tag urgent,backend
bea update <id> --title "New title" --body "Updated description"
```

### `bea epics`
List all epics with progress (done/total children).

```sh
bea epics
```

### `bea start` / `bea done` / `bea cancel`
Shortcuts for the most common status transitions:

```sh
bea start <id>    # → in_progress
bea done <id>     # → done
bea cancel <id>   # → cancelled
```

When all children of an epic are completed, the epic is automatically marked as done.

### `bea dep`
```sh
bea dep add <id> <depends-on-id>    # add dependency (cycle-safe)
bea dep remove <id> <depends-on-id>
bea dep tree <id>                   # show dependency tree
```

Adding a dependency that would create a cycle is rejected with an error.

### `bea delete`
Permanently delete a task file.

```sh
bea delete <id>
```

### `bea prune`
Delete cancelled tasks. Use `--done` to also delete completed tasks.

```sh
bea prune
bea prune --done
```

### `bea graph`
Show the dependency graph as a tree. Hides `done` and `cancelled` tasks by default; use `--all` / `-a` to include them.

```sh
bea graph
bea graph --all
```

### `bea search`
Matches against title, body, tags, and ID. Hides `done` and `cancelled` tasks by default.

```sh
bea search "oauth"
bea search "oauth" --all
```

### `bea edit`
Open a task's `.md` file in your `$EDITOR` for direct editing. Falls back to `$VISUAL`, then `vi`. After the editor exits, the file is re-parsed and validated.

```sh
bea edit <id>
```

### `bea completions`
Generate shell completions for bash, zsh, or fish.

```sh
bea completions bash
bea completions zsh
bea completions fish
```

Add to your shell config to enable completions:

```sh
# zsh — add to .zshrc
eval "$(bea completions zsh)"

# bash — add to .bashrc
eval "$(bea completions bash)"

# fish — add to ~/.config/fish/config.fish
bea completions fish | source
```

---

## JSON output

Every command accepts `--json` for machine-readable output:

```sh
bea --json list
bea --json ready --limit 3
bea --json create "New task" --priority P1
```

---

## MCP server

`bears` can run as an [MCP](https://modelcontextprotocol.io) server, exposing all task operations as tools for AI coding agents.

```sh
bea mcp   # starts MCP server over stdio
```

### Available MCP tools

| Tool | Description |
|---|---|
| `list_ready` | Tasks ready to work on (`limit?`, `tag?`, `epic?`) |
| `list_all_tasks` | All tasks with optional filters (`status?`, `priority?`, `tag?`, `epic?`) |
| `list_epics` | List all epics with progress |
| `get_task` | Full task details (`id`) |
| `create_task` | Create a task or epic (`title`, `priority?`, `tags?`, `depends_on?`, `body?`, `type?`) |
| `update_task` | Update fields (`id`, `status?`, `priority?`, `tags?`, `assignee?`, `body?`) |
| `start_task` | Set status to `in_progress` (`id`) |
| `complete_task` | Set status to `done` (`id`) |
| `cancel_task` | Set status to `cancelled` (`id`) |
| `prune_tasks` | Delete cancelled tasks (`include_done?`) |
| `add_dependency` | Add a dependency, cycle-safe (`id`, `depends_on`) |
| `remove_dependency` | Remove a dependency (`id`, `depends_on`) |
| `delete_task` | Permanently delete a task (`id`) |
| `search_tasks` | Full-text search (`query`) |
| `get_graph` | Adjacency list of all dependencies |

### Register with Claude Code

Add to your Claude Code MCP config (`claude mcp add`):

```json
{
  "mcpServers": {
    "bears": {
      "command": "bea",
      "args": ["mcp"]
    }
  }
}
```

---

## Development

```sh
cargo build
cargo test
cargo clippy
cargo fmt
```

All three of `fmt`, `clippy`, and `test` must pass cleanly before committing.

---

## Project layout

```
src/
  main.rs        Entry point — dispatch to CLI or MCP server
  cli/
    mod.rs       CLI module root and dispatch
    args.rs      clap command and argument definitions
    cmd.rs       Command handlers (list, show, create, etc.)
  mcp/
    mod.rs       MCP module root and server setup
    params.rs    Tool parameter structs (serde + JSON Schema)
    tools.rs     MCP tool implementations and tests
  service.rs     Business logic (create, update, epic progress, etc.)
  store.rs       Read/write .bears/ directory
  task.rs        Task struct, frontmatter parse/render, ID & slug
  graph.rs       Dependency graph, ready computation, cycle detection
  config.rs      .bears.yml configuration
  error.rs       Error types
.bears/          Task files (created by `bea init`)
```
