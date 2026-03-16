# Copilot Instructions for `bea-rs`

Use this file as the source of truth for AI-assisted edits in this repository.

## Project Summary

`bea-rs` is a file-based task tracker CLI (`bea`) managing a task graph stored as Markdown files with YAML frontmatter in `.bears/`.

It has two modes:
- CLI mode: `bea <command>`
- MCP server mode: `bea mcp` (stdio tool server)

## Development Workflow

Work test-driven: add or update tests before (or alongside) implementation.
After finishing any task, always run and ensure all pass:

Project planning and task tracking use Bears. Prefer managing tasks through MCP tools when available instead of direct CLI usage.
- Find next task using `list_ready` (MCP) or `bea ready` (CLI).
- Start a task with `start_task` (MCP) or `bea start <id>` (CLI).
- When done, mark complete with `complete_task` (MCP) or `bea complete <id>` (CLI).


```bash
cargo fmt && cargo clippy && cargo test
```

## Commands

```bash
cargo build
cargo build --release
cargo run -- <args>
cargo test
cargo test <test_name>
cargo clippy
cargo fmt
cargo add <crate>
```

Always add dependencies with `cargo add` rather than manually editing `Cargo.toml`.

## Architecture

- `src/main.rs`: entry point; dispatches CLI vs MCP server
- `src/cli/mod.rs`: CLI module root and dispatch
- `src/cli/args.rs`: clap command and argument definitions
- `src/cli/cmd.rs`: CLI command handlers
- `src/mcp/mod.rs`: MCP module root and server setup
- `src/mcp/params.rs`: tool parameter structs (serde + JSON Schema)
- `src/mcp/tools.rs`: MCP tool implementations and tests
- `src/service.rs`: business logic (create, update, epic progress, auto-close)
- `src/store.rs`: core task storage and file parsing/writing
- `src/task.rs`: task model, frontmatter serde, ID generation
- `src/graph.rs`: dependency graph, ready query, cycle detection
- `src/config.rs`: `.bears.yml` configuration loading
- `src/error.rs`: thiserror-based error types

Design principles:
- Keep `store.rs`, `service.rs`, and `graph.rs` as core logic; CLI/MCP should remain thin frontends.
- Re-parse `.bears/` on each invocation (no cache, no daemon).
- Keep CLI `--json` behavior intact for all command outputs.
- MCP responses should stay minimal and structured (avoid full markdown body unless required).

## Task Storage Rules

Task files are stored as `.bears/{id}-{slug}.md` with YAML frontmatter.

Expected fields:
- `id`: short lowercase alphanumeric (configurable length, default 3 chars)
- `title`
- `status`: `open | in_progress | done | blocked | cancelled`
- `priority`: `P0 | P1 | P2 | P3`
- `type`: `task | epic` (defaults to `task`, omitted from frontmatter when `task`)
- `created`, `updated` (timestamps)
- `tags`: list
- `depends_on`: list of task IDs
- `parent`: optional task/epic ID
- `assignee`: string
- body: markdown content after frontmatter

Frontmatter parsing approach:
- Split on `---` delimiters
- Parse YAML with `serde_yaml`
- Preserve trailing markdown as body string

## Behavior Requirements

### ID generation
- Generate short lowercase alphanumeric IDs (configurable length via `.bears.yml`, default 3 chars).
- Check collisions against existing tasks; regenerate if needed.

### Ready task logic
- `ready` includes tasks with `status == open`, `type == task`, and all dependencies in `done`.
- Epics are excluded from ready results (they are tracked separately).
- Sort by priority (`P0` first), then creation date.

### Epic behavior
- Epics group child tasks via the `parent` field.
- `bea epics` / `list_epics` shows epics with progress (done/total children).
- When all children of an epic are completed, the epic is automatically marked as done.
- Epics are excluded from `ready` results.

### Dependency safety
- Validate `dep add` operations do not create cycles.
- Reject cycle-forming edges with a clear error.

## MCP Tool Surface

Keep these tools aligned with implementation and schemas:
- `list_ready(limit?, tag?, epic?)`
- `list_all_tasks(status?, priority?, tag?, epic?)`
- `list_epics()`
- `get_task(id)`
- `create_task(title, priority?, tags?, depends_on?, parent?, body?, type?)`
- `update_task(id, status?, priority?, tags?, assignee?, body?)`
- `start_task(id)`
- `complete_task(id)`
- `cancel_task(id)`
- `add_dependency(id, depends_on)`
- `remove_dependency(id, depends_on)`
- `delete_task(id)`
- `search_tasks(query)`
- `prune_tasks(include_done?)`
- `get_graph()`

## Error Handling Expectations

- Missing `.bears/` should provide actionable guidance (suggest `bea init`).
- Unknown task IDs should produce clear errors.
- Cycle detection errors should explain why the edge is rejected.
- Invalid frontmatter should warn-and-skip problematic files, not crash the process.
- Library code should use `Result` + `?`; CLI formats user-facing errors; MCP methods map to `rmcp::ErrorData`.

## Dependencies

Use and keep dependency scope minimal:
- `clap`, `clap_complete`, `serde`, `serde_yaml`, `serde_json`, `chrono`, `rand`, `thiserror`, `tokio`, `rmcp`, `schemars`, `owo-colors`

Prefer small, compile-fast changes and avoid introducing heavy dependencies without need.

## Testing Guidance

Maintain or extend coverage in:
- unit tests for graph algorithms (`graph.rs`)
- parser/storage tests (`task.rs`, `store.rs`)
- service logic tests (`service.rs`)
- MCP tool tests (`mcp/tools.rs`)
- integration tests using temp `.bears/` directories

MCP layer is validated by both unit tests in `mcp/tools.rs` and end-to-end tool usage.

## Commit Message Style

Use Conventional Commits:
- `feat(scope): description`
- `fix(scope): description`
- `refactor(scope): description`
- `test(scope): description`
- `docs(scope): description`
- `chore(scope): description`
