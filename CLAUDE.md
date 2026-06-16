# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

`bea-rs` is a file-based task tracker CLI tool named `bears` (binary: `bea`). It manages a task/issue graph stored as markdown files with YAML frontmatter in a `.bears/` directory. It has two modes:

1. **CLI mode** (`bea <command>`): Human-friendly interface for managing tasks
2. **MCP server mode** (`bea mcp`): Exposes the same functionality as MCP tool calls over stdio for AI agents
3. **Interactive TUI** (`bea tui`): A full-screen `ratatui` terminal UI for browsing and managing tasks, with live refresh when `.bears/` changes on disk

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
  main.rs          # Entry point: dispatch to CLI, MCP server, or TUI
  cli/
    mod.rs         # CLI module root and dispatch
    args.rs        # clap command and argument definitions
    cmd.rs         # Command handlers (list, show, create, edit, graph, etc.)
  mcp/
    mod.rs         # MCP module root and server setup
    params.rs      # Tool parameter structs (serde + JSON Schema)
    tools.rs       # MCP tool implementations and tests
  tui/             # Interactive ratatui terminal UI (`bea tui`)
    mod.rs         # TUI event loop and app wiring
    app.rs         # App state, filtering (reuses graph::is_task_ready), key handling
    input.rs       # Key/event input handling
    style.rs       # Theme and colors
    watcher.rs     # Debounced .bears/ file watcher (notify) for live refresh
    widgets/       # task list/detail/info, dep tree, modals, body, bottom bar
  service.rs       # Business logic: create, update, reparent, epic progress, auto-close
  store.rs         # Core: parse .bears/ dir, read/write task files, archive layer
  task.rs          # Task struct, frontmatter serde, enums, ID generation
  graph.rs         # Dependency graph: build, ready, effective priority, cycle detection, dep tree
  scaffold.rs      # `bea init` harness-integration scaffolding (Claude/Copilot/Codex)
  config.rs        # .bears.yml configuration loading
  editor.rs        # $EDITOR integration for `bea edit`
  error.rs         # thiserror error types
templates/         # Embedded harness templates (via include_str!) for init scaffolding
```

### Core design principles

- `store.rs`, `service.rs`, and `graph.rs` are the core library. CLI, MCP, and TUI are thin frontends.
- Re-parse the entire `.bears/` directory from scratch on every invocation — no caching, no daemon.
- `--json` flag on all CLI commands outputs JSON instead of human text.
- MCP tools return minimal structured data (id, title, priority, status, tags) — not full markdown bodies.
- Effective-priority and ready computation are single-pass O(V+E); dep-tree rendering expands each node once (DAG, not exponential). `get_graph` returns a *bounded* adjacency list (excludes done/cancelled and isolated nodes) to keep agent payloads small.

### `bea init` harness flags

`bea init` accepts one or more optional harness flags that scaffold coding-agent integration files into the project root. Flags may be combined.

| Flag | Files scaffolded | MCP registration |
|------|-----------------|-----------------|
| `--claude` | `CLAUDE.md`, `.claude/skills/bears-planning/SKILL.md`, `.claude/skills/bears-planning/references/cli-fallback.md`, `.claude/agents/planner.md` | `.mcp.json` (merged, preserves existing servers) |
| `--copilot` | `.github/copilot-instructions.md`, `.github/skills/bears-planning/SKILL.md`, `.github/skills/bears-planning/references/cli-fallback.md`, `.github/agents/planner.agent.md` | `.github/mcp.json` (merged) |
| `--codex` | `AGENTS.md` | none (Codex discovers servers another way) |

`bea init` also accepts `--force` to overwrite existing files without prompting.

**Overwrite guard.** If any target file already exists, `bea init` asks once —
*"This directory already contains agent instructions and/or skills. Overwrite them? (y/N)"* — defaulting to **No**. On No, existing files are left untouched but any missing files are still created and `.mcp.json` still merges. When stdin is not a terminal (CI, pipes), it never prompts and behaves as No (skip existing). `--force` overwrites unconditionally. The `.mcp.json` merge always runs regardless (it's safe — only the `bears` key is touched).

### `bea agent <category>` — fine-grained scaffolding

`bea agent <instructions|skills|all> --claude/--copilot/--codex [--force] [--append]` scaffolds a subset of agent files **without** creating `.bears/`.

| Category | Files |
|----------|-------|
| `instructions` | the top-level instruction file only (`CLAUDE.md` / `AGENTS.md` / `copilot-instructions.md`) |
| `skills` | skill + references + planner-agent files, plus the `.mcp.json` merge |
| `all` | everything (same as `bea init`'s scaffolding) |

- `--force` overwrites existing files without prompting.
- `--append` appends template content to the existing instruction file (under an `<!-- bears:begin -->` marker, idempotent). Only valid for `instructions`/`all`; `--append` with `skills` is a usage error. `--force` and `--append` are mutually exclusive.
- With neither flag, the same single y/N overwrite prompt as `bea init` applies (skip when non-interactive).

Key invariants:
- Scaffolding is **idempotent**: re-running with `--force` (or via the back-compat `scaffold()` wrapper) re-writes the same files with the same content; the default path preserves existing files.
- MCP merge **preserves unrelated servers**: only the `bears` key is inserted/replaced; all other entries in the server map are left intact.
- Generated `.mcp.json` / `.github/mcp.json` always uses `{ "command": "bea", "args": ["mcp"] }` — never `cargo run`.
- Template files live in `templates/` under the crate root and are embedded via `include_str!` in `scaffold.rs`. They must be present in the source tree for `cargo package` to include them.

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

Parse frontmatter by splitting on `---` delimiters, using `serde_yml` for the YAML portion, keeping the rest as the body string.

### ID generation

Generate a short lowercase alphanumeric ID (configurable length via `.bears.yml`, default 3 chars). Check for collisions against both active and archived tasks; regenerate if needed.

### `ready` command

The key command for agent workflows. Returns tasks where status is `open`, type is `task` (not `epic`), AND all `depends_on` tasks have status `done`. Sort by priority (P0 first), then creation date.

### Epic behavior

Epics group child tasks via the `parent` field (set at create time, or changed later via `update --parent <id|"">` / the `parent` field on `update_task`). `bea epics` / `list_epics` shows epics with progress (done / total non-cancelled children). An epic auto-closes when every child is *resolved* — i.e. `done` **or** `cancelled` (cancelled children are non-blocking and excluded from the total). Auto-close runs on both the `set_status` and `update_task` status paths and cascades up through nested epics. Epics are excluded from `ready` results.

### Cycle detection

Validate on `dep add` that the new edge doesn't create a cycle. Reject with a clear error if it would.

### Archive

Archived tasks live in `.bears/archive/` — a subdirectory created by `bea init`. `store::load_all` scans only the top-level `.bears/` directory and therefore **never** returns archived tasks; the archive is invisible to all normal operations.

**Archivability invariant.** A task is archivable if and only if:
1. Its status is `done` or `cancelled` (terminal), **and**
2. No active (not done/cancelled) task in the active store depends on it.

Violating either condition causes `archive_task` to return `Error::NotArchivable` naming the blockers.

**Cascade direction.**
- *Archiving an epic* automatically also archives its settled (done/cancelled) children.
- *Restoring a task* automatically also restores any archived `depends_on` tasks (transitively) and the archived parent epic, so the restored task has no missing dependencies.

**CLI commands.**

| Command | Description |
|---------|-------------|
| `bea archive <id>` | Archive a single task (and its settled epic children). Fails if active dependents exist. |
| `bea archive` | Sweep: archive every currently-archivable task (fixed-point iteration). |
| `bea restore <id>` | Restore a task (and its cascade of archived deps/parent) back to active. |
| `bea list --archived` | List all archived tasks. |
| `bea log [--limit N]` | Show archived tasks sorted by `updated` descending (most-recent first). |

**MCP tools.**

| Tool | Key params | Notes |
|------|-----------|-------|
| `archive_task` | `id?` | Omit `id` to sweep all archivable tasks |
| `restore_task` | `id` | Restores cascade of deps and parent epic |
| `list_archived` | `limit?` | Returns summaries sorted most-recent-first |

Archived tasks are **hidden** from all default-listing tools (`list_all_tasks`, `list_ready`, `search_tasks`, `get_graph`, `list_epics`). They are visible only via `list_archived`.

**`prune` is hard-delete; archive is the soft option.** `prune_tasks` / `bea prune` permanently deletes active cancelled (and optionally done) tasks. It never touches the archive. Use archive/restore when history should be recoverable.

**ID collision prevention.** `create_task` excludes both active and archived IDs from the candidate pool so new IDs never collide with archived ones.

### MCP tools

| Tool | Key params |
|------|------------|
| `list_ready` | `limit?`, `tag?`, `epic?` |
| `list_all_tasks` | `status?`, `priority?`, `tag?`, `epic?`, `limit?`, `active_only?` |
| `list_epics` | — |
| `get_task` | `id` |
| `create_task` | `title`, `priority?`, `tags?`, `depends_on?`, `parent?`, `body?`, `type?` |
| `update_task` | `id`, `title?`, `status?`, `priority?`, `tags?`, `assignee?`, `body?`, `parent?` |
| `start_task` | `id` |
| `complete_task` | `id` |
| `cancel_task` | `id` |
| `prune_tasks` | `include_done?` |
| `add_dependency` | `id`, `depends_on` |
| `remove_dependency` | `id`, `depends_on` |
| `delete_task` | `id` |
| `search_tasks` | `query`, `limit?`, `active_only?` |
| `plan_epic` | `id` |
| `get_graph` | `include_done?`, `epic?`, `limit?` |
| `archive_task` | `id?` |
| `restore_task` | `id` |
| `list_archived` | `limit?` |

On `update_task`, an empty-string `parent` (`""`) clears the parent; omitting it leaves the parent unchanged. `active_only?` (on `list_all_tasks` / `search_tasks`) excludes done/cancelled tasks. `get_task` resolves active tasks first and **falls back to the archive** when the id isn't active, marking the returned detail with `"archived": true` (read-only — restore before mutating).

## Dependencies

- `clap` (derive) — CLI parsing
- `clap_complete` — shell completion generation
- `serde`, `serde_yml`, `serde_json` — serialization
- `chrono` — timestamps
- `rand` — ID generation
- `thiserror` — error types
- `tokio` — async runtime used for MCP server, parallel file loading in `store::load_all`
- `rmcp` — MCP SDK (server, macros, transport-io features)
- `schemars` — JSON Schema generation for MCP tool parameter types
- `owo-colors` — terminal color output
- `ratatui`, `crossterm` — interactive TUI rendering and terminal/event handling
- `tui-markdown` — markdown rendering inside the TUI
- `notify`, `notify-debouncer-mini` — debounced `.bears/` filesystem watching for live TUI refresh
- `shell-words` — parse the `$EDITOR` command for `bea edit`

Keep the dependency tree small. Compilation should be fast.

## Error handling

- `.bears/` not found → suggest `bea init`
- Task ID not found → clear error
- Cycle detected → reject with explanation
- Invalid frontmatter → warn and skip (don't crash), report which file
- Library code uses `Result` with `?`. CLI formats errors for humans. MCP returns `rmcp::ErrorData` from tool methods.

## Testing

- Unit tests in `graph.rs`: cycle detection, topological sort, ready computation
- Unit tests in `task.rs`/`store.rs`: frontmatter parsing (valid, missing fields, extra fields, malformed); archive storage layer (`move_to_archive`, `move_from_archive`, `load_archived`, `all_known_ids`)
- Unit tests in `service.rs`: epic progress, auto-close (incl. cancelled children and nested cascade), reparenting; archive service (`is_archivable`, `archive_task`, `archive_all`, `restore_task`, `list_archive`, `get_archived_task`, archived-ID collision avoidance)
- Unit tests in `mcp/tools.rs`: tool create/list/start/complete/search/graph/plan_epic/delete/deps/validation; archive tools (archive/restore/list_archived); end-to-end archive visibility (hidden from list/search/graph/epics), integrity (dep add onto archived ID, prune never touches archive, no ID reuse)
- Unit tests in `graph.rs`: effective-priority correctness, DAG dep-tree bounding, bounded adjacency, plus `#[ignore]`d coupled-graph perf benchmarks (run with `cargo test -- --ignored`)
- Unit tests in `scaffold.rs`: `.mcp.json` merge (preserve/idempotent/fresh) and harness scaffolding (claude/copilot/codex skill/agent files, `bea mcp` binary form)
- Unit tests in `tui/`: input truncation (UTF-8 safety), ready-filter parity, watcher (the watcher test is `#[ignore]`d as timing-sensitive)
- Integration tests in `tests/cli.rs`: create a temp `.bears/` dir, run commands, verify file output; archive/restore/log/list-archived e2e; `bea init --claude/--copilot/--codex` scaffolding e2e (file presence, idempotency, MCP merge, `bea mcp` binary form, template packaging guard)
- E2E tests in `tests/mcp.rs`: spawn the real `bea mcp` binary and drive it over stdio with the
  rmcp client (dev-dependency features `client`, `transport-child-process`) — handshake, tool
  list/schemas, tool calls, and protocol- vs tool-level error semantics
- MCP tools also verified end-to-end via live MCP sessions
