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

### Homebrew

```sh
brew install lhelge/tap/bears
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
bea review <id>     # done working — hand it to a reviewer
bea done <id>
bea release <id>    # stuck? hand it back to the pool
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
assignee: alice
attempts: 2
---

Any Markdown body goes here.
```

Optional fields (`tags`, `depends_on`, `parent`, `assignee`, `attempts`) are omitted when empty — a missing `attempts` counts as `0`.

**Statuses:** `proposed` · `open` · `in_progress` · `review` · `done` · `blocked` · `cancelled`

A task normally travels `open → in_progress → review → done`. `proposed` is the waiting room in
front of the backlog: proposals are never ready and are hidden from `bea list` until accepted.
`review` is finished work waiting for a second pair of eyes — it is not ready work either, and it
does **not** unblock dependents, so nothing gets built on top of unreviewed code.

**Types:** `task` (default) · `epic` (high-level objective grouping child tasks)

**Priorities:** `P0` (critical) · `P1` · `P2` · `P3` (low) — sorted P0 first everywhere. A task inherits the highest priority of any task that depends on it, so a P3 task blocking a P0 task is effectively treated as P0.

---

## Commands

### `bea init`
Create the `.bears/` directory and `.bears.yml` config in the current directory.

```sh
bea init
```

Optionally scaffold coding-agent integration files with one or more harness flags:

```sh
bea init --claude    # CLAUDE.md + .mcp.json + .claude/skills/ + .claude/agents/
bea init --copilot   # .github/copilot-instructions.md + .github/mcp.json + .github/skills/ + .github/agents/
bea init --codex     # AGENTS.md
bea init --claude --copilot   # combine flags
```

Flags are **idempotent**: re-running them on an already-initialized directory is safe and refreshes the files. When merging `.mcp.json` / `.github/mcp.json`, any pre-existing unrelated server entries are preserved. The generated MCP server entry always uses `bea mcp` (not `cargo run`).

### `bea create`
```sh
bea create "Title" [--priority P0-P3] [--tag tag1,tag2] [--depends-on id1,id2] [--body "..."] [--epic]
```

Use `--epic` to create an epic instead of a regular task. Epics are high-level objectives that group child tasks via the `--parent` flag.

### `bea list`
Hides `done`, `cancelled` and `proposed` tasks by default. Use `--all` / `-a` to show everything, or
`--status proposed` to see just the proposal queue.

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

`proposed` and `review` tasks never appear here: a proposal has not been accepted yet, and a task in
review is finished work needing a reviewer, not new work. See `bea review` for that queue.

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
bea update <id> --parent <epic-id>    # reparent under an epic (use "" to detach)
```

### `bea epics`
List all epics with progress (done/total children).

```sh
bea epics
```

### `bea start` / `bea done` / `bea cancel` / `bea release`
Shortcuts for the most common status transitions:

```sh
bea start <id>                    # → in_progress
bea start <id> --assignee alice   # → in_progress, claimed by alice
bea done <id>                     # → done
bea cancel <id>                   # → cancelled
bea release <id>                  # → open, assignee cleared
```

`--assignee` / `-a` records who (or which agent) is working on the task; omitting it leaves any
existing assignee untouched, and `--assignee ""` clears it.

`attempts` counts how many times work has been started on a task. It increments on every transition
into `in_progress` (`bea start`, `bea status <id> in_progress`, `bea update --status in_progress`,
or the TUI) — re-running `bea start` on a task you already hold is the same attempt, so handing it to
a different assignee does not inflate the count. Releasing or completing a task never changes it, so
a task that succeeded on the third try reads `attempts: 3`.

`bea release` hands a claimed task back to the pool — the status returns to `open` and the assignee
is cleared, so the task shows up in `bea ready` again for another worker. It applies **only** to
`in_progress` tasks: releasing anything else is an error, so finished work is never silently
reopened. Use it when a worker gets stuck or is killed mid-task.

When all children of an epic are completed, the epic is automatically marked as done.

### `bea review` / `bea reject`
Review is the step between "I finished it" and "it's done":

```sh
bea review <id>   # in_progress → review, submit your work
bea review        # list everything awaiting review
bea reject <id>   # review → open, changes needed (assignee kept)
bea done <id>     # review → done, approved
```

`bea review` with no ID prints the review queue, sorted highest-priority first, with the assignee of
each task so you can see who wrote it. It accepts the same `--tag`, `--epic` and `--limit` filters as
`bea ready`. The queue is deliberately separate from `bea ready` — reviewing someone else's work and
starting fresh work are different jobs, so an orchestrator can dispatch them to different workers.

`bea reject` keeps the assignee, so a rejected task carries who wrote it and rework can be routed
back to the same person. Use `bea release` instead to hand it back to the pool entirely.

### `bea propose` / `bea accept`
A proposal is work someone suggests but that has not been accepted into the backlog:

```sh
bea propose <id>   # open → proposed
bea accept <id>    # proposed → open
bea list --status proposed
```

Proposals never show up in `bea ready` or in a default `bea list`, so an agent can file ideas
without drowning the real backlog.

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
Permanently delete cancelled tasks. Use `--done` to also delete completed tasks. For **recoverable** cleanup, prefer `bea archive`.

```sh
bea prune
bea prune --done
```

### `bea archive` / `bea restore` / `bea log`
Move settled work out of the active set into `.bears/archive/`, keeping `list`/`ready`/`search`/`graph`/`epics` focused on active tasks. Unlike `prune`, archiving is **reversible**.

```sh
bea archive            # sweep: archive every archivable task
bea archive <id>       # archive one task (and its settled epic children)
bea restore <id>       # bring an archived task (and its deps + parent epic) back
bea list --archived    # list archived tasks
bea log [--limit N]    # archived tasks, most recently archived first
```

A task is **archivable** only when it is `done`/`cancelled` **and** no active task still depends on it (otherwise archiving is refused, naming the blockers). Archived tasks are hidden from all normal listings; `bea show <id>` still finds them (labelled as archived), but mutating an archived task is refused until you `restore` it.

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

## Interactive TUI

```sh
bea tui
```

A full-screen terminal UI for browsing and managing tasks, with **live refresh** when `.bears/` changes on disk (e.g. while an AI agent edits tasks in the background). Press `m` to cycle the list view — **Open**, **Ready**, **Epics**, **Completed** (done/cancelled still in the active set), **Archive** (loaded from `.bears/archive/`), **All** — `/` to filter by text, and edit the selected task in your `$EDITOR`. The detail pane shows a task's direct dependencies, and for epics the full subtask tree.

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
| `list_all_tasks` | All tasks with optional filters (`status?`, `priority?`, `tag?`, `epic?`, `limit?`, `active_only?`, `include_proposed?`) |
| `list_epics` | List all epics with progress |
| `get_task` | Full task details (`id`); falls back to the archive, marking the result `archived: true` |
| `create_task` | Create a task or epic (`title`, `priority?`, `tags?`, `depends_on?`, `parent?`, `body?`, `type?`) |
| `update_task` | Update fields (`id`, `title?`, `status?`, `priority?`, `tags?`, `assignee?`, `body?`, `parent?`) |
| `start_task` | Set status to `in_progress`, optionally claiming it (`id`, `assignee?`) |
| `list_review` | Tasks awaiting review, separate from `list_ready` (`limit?`, `tag?`, `epic?`) |
| `review_task` | Submit in-progress work for review (`id`) |
| `reject_task` | Send a task under review back for changes (`id`) |
| `propose_task` | Demote an open task to a proposal (`id`) |
| `accept_task` | Accept a proposal into the backlog (`id`) |
| `release_task` | Release an in-progress task: status → `open`, assignee cleared (`id`) |
| `complete_task` | Set status to `done` (`id`) |
| `cancel_task` | Set status to `cancelled` (`id`) |
| `prune_tasks` | Permanently delete cancelled tasks (`include_done?`) |
| `add_dependency` | Add a dependency, cycle-safe (`id`, `depends_on`) |
| `remove_dependency` | Remove a dependency (`id`, `depends_on`) |
| `delete_task` | Permanently delete a task (`id`) |
| `search_tasks` | Full-text search (`query`, `limit?`, `active_only?`) |
| `plan_epic` | An epic's child tasks in topological execution order (`id`) |
| `get_graph` | Bounded dependency adjacency list (`include_done?`, `epic?`, `limit?`) |
| `archive_task` | Archive a task (and settled children), or sweep all archivable tasks (`id?`) |
| `restore_task` | Restore an archived task and its cascade (`id`) |
| `list_archived` | List archived tasks, most recent first (`limit?`) |

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

## Library usage

The core is published as a library crate so an independent application can
drive a bears repository directly, with no `bea` binary required on the user's
machine.

The package is `bea-rs`; the library is `bears`:

```toml
[dependencies]
bears = { package = "bea-rs", version = "0.8", default-features = false }
```

`default-features = false` drops the CLI, TUI, and MCP frontends — the library
then pulls in ~33 crates instead of ~180.

```rust
use bears::{Priority, Status, service, store, task::TaskType};

let base = std::path::Path::new(".");
store::init(base)?;

// Every call re-reads the directory — there is no cache and no daemon.
let tasks = store::load_all(base).await?;
let task = service::create_task(
    base,
    &tasks,
    "Design the API".to_string(),
    Priority::P0,
    vec!["backend".to_string()],   // tags
    vec![],                        // depends_on
    None,                          // parent epic
    String::new(),                 // markdown body
    TaskType::Task,
)?;

let tasks = store::load_all(base).await?;
for t in service::list_ready(&tasks, None, None, None) {
    println!("{} {} {}", t.id, t.priority, t.title);
}

service::set_status(base, &tasks, &task.id, Status::Done)?;
```

### Layout

| Module | What it does |
|--------|--------------|
| `store` | Parse and write the `.bears/` directory, including the archive |
| `task` | `Task`, the frontmatter format, ID and slug generation |
| `graph` | Dependency graph, readiness, effective priority, cycle detection |
| `service` | Business logic — create, update, reparent, archive, epic progress and auto-close |
| `scaffold` | Write coding-agent integration files (`CLAUDE.md`, skills, MCP config) |
| `error` | `Error` and `Result` |

Most callers want `service`. It takes `&HashMap<String, Task>` from
`store::load_all` and a `base` path, and writes changes straight to disk.

### Features

| Feature | Default | Effect |
|---------|---------|--------|
| `cli` | yes | Builds the `bea` binary: CLI, MCP server, and TUI |
| `schema` | via `cli` | Derives `schemars::JsonSchema` on `Status`, `Priority`, and `TaskType` — useful when exposing bears types in your own tool schemas |

### Errors

Library errors carry no frontend suggestions. `Error::NotInitialized` renders
as ``not initialized: no `.bears` directory found`` — it is up to your frontend
to add whatever remediation hint fits. The `bea` binary attaches its own hints
in `hint_for()` in `main.rs`.

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
  lib.rs         Library root — public API (store, task, graph, service, ...)
  main.rs        `bea` binary entry point — dispatch to CLI, MCP server, or TUI
  cli/
    mod.rs       CLI module root and dispatch
    args.rs      clap command and argument definitions
    cmd.rs       Command handlers (list, show, create, edit, graph, etc.)
  mcp/
    mod.rs       MCP module root and server setup
    params.rs    Tool parameter structs (serde + JSON Schema)
    tools.rs     MCP tool implementations and tests
  tui/           Interactive ratatui terminal UI (widgets, watcher, input)
  service.rs     Business logic (create, update, reparent, epic auto-close, archive)
  store.rs       Read/write .bears/ directory, incl. the archive layer
  task.rs        Task struct, frontmatter parse/render, ID & slug
  graph.rs       Dependency graph, ready computation, cycle detection
  scaffold.rs    `bea init` harness scaffolding (Claude/Copilot/Codex)
  config.rs      .bears.yml configuration
  editor.rs      $EDITOR integration for `bea edit` (binary-only)
  error.rs       Error types
templates/       Embedded harness templates for init scaffolding
.bears/          Task files (created by `bea init`)
  archive/       Archived task files
```
