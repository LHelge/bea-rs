# bears 🐻

A file-based task tracker for developers and AI agents.

Tasks live as Markdown files with YAML frontmatter in a `.tasks/` directory — plain text, git-friendly, no database. Run `bea` from the terminal or expose the same functionality as an MCP server for AI coding agents.

---

## Install

```sh
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

Each task is stored as `.tasks/{id}-{slug}.md`:

```markdown
---
id: a1b2
title: Implement OAuth flow
status: open
priority: P1
created: 2026-03-15T10:30:00Z
updated: 2026-03-15T10:30:00Z
tags: [backend, auth]
depends_on: [f4c9]
---

Any Markdown body goes here.
```

**Statuses:** `open` · `in_progress` · `done` · `blocked` · `cancelled`

**Priorities:** `P0` (critical) · `P1` · `P2` · `P3` (low) — sorted P0 first everywhere.

---

## Commands

### `bea init`
Create the `.tasks/` directory in the current directory.

```sh
bea init
```

### `bea create`
```sh
bea create "Title" [--priority P0-P3] [--tag tag1,tag2] [--depends-on id1,id2] [--body "..."]
```

### `bea list`
```sh
bea list
bea list --status open
bea list --priority P0
bea list --tag backend
```

### `bea ready`
Show tasks that are `open` and have all dependencies completed. This is the key command for agent workflows — always start here.

```sh
bea ready
bea ready --tag backend --limit 5
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

### `bea start` / `bea done`
Shortcuts for the most common status transitions:

```sh
bea start <id>   # → in_progress
bea done <id>    # → done
```

### `bea dep`
```sh
bea dep add <id> <depends-on-id>    # add dependency (cycle-safe)
bea dep remove <id> <depends-on-id>
bea dep tree <id>                   # show dependency tree
```

Adding a dependency that would create a cycle is rejected with an error.

### `bea graph`
Show the full dependency graph for all tasks.

### `bea search`
```sh
bea search "oauth"
```
Matches against title, body, tags, and ID.

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
| `list_ready` | Tasks ready to work on (`limit?`, `tag?`) |
| `list_all_tasks` | All tasks with optional filters (`status?`, `priority?`, `tag?`) |
| `get_task` | Full task details (`id`) |
| `create_task` | Create a task (`title`, `priority?`, `tags?`, `depends_on?`, `body?`) |
| `update_task` | Update fields (`id`, `status?`, `priority?`, `tags?`, `assignee?`, `body?`) |
| `start_task` | Set status to `in_progress` (`id`) |
| `complete_task` | Set status to `done` (`id`) |
| `add_dependency` | Add a dependency, cycle-safe (`id`, `depends_on`) |
| `remove_dependency` | Remove a dependency (`id`, `depends_on`) |
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
  main.rs    Entry point — dispatch to CLI or MCP server
  cli.rs     CLI commands (clap)
  mcp.rs     MCP server — JSON-RPC 2.0 over stdio
  store.rs   Read/write .tasks/ directory
  task.rs    Task struct, frontmatter parse/render, ID & slug
  graph.rs   Dependency graph, ready computation, cycle detection
  error.rs   Error types
.tasks/      Task files (created by `bea init`)
```
