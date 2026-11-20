# Copilot Instructions for `bea-rs`

Use this file as the source of truth for AI-assisted edits in this repository.

## Project Summary

`bea-rs` is a file-based task tracker CLI (`bea`) managing a task graph stored as Markdown files with YAML frontmatter in `.bears/`.

It has three modes:
- CLI mode: `bea <command>`
- MCP server mode: `bea mcp` (stdio tool server)
- Interactive TUI: `bea tui`

The crate ships a library (`bears`, the core) and the `bea` binary (CLI, MCP, TUI frontends). `CLAUDE.md` holds the full, current design notes; follow it where this summary is shorter.

## Development Workflow

Work test-driven: add or update tests before (or alongside) implementation.
After finishing any task, always run and ensure all pass:

Project planning and task tracking use Bears. Prefer managing tasks through MCP tools when available instead of direct CLI usage.
- Find next task using `list_ready` (MCP) or `bea ready` (CLI).
- Start a task with `start_task` (MCP) or `bea start <id>` (CLI); pass `assignee` / `--assignee` to claim it.
- If you get stuck on a claimed task, hand it back with `release_task` (MCP) or `bea release <id>` (CLI) — status returns to `open` and the assignee is cleared.
- When work needs a second pair of eyes, `review_task` / `bea review <id>` instead of completing it. Reviewers pull from `list_review` / `bea review` and either complete or `reject_task` it.
- Tasks carry an `attempts` count that increments on each start. A high count means previous workers failed — change approach or escalate.
- When done, mark complete with `complete_task` (MCP) or `bea done <id>` (CLI).


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

- `src/lib.rs`: library root (`bears`); the core never references `cli`, `mcp` or `tui`
- `src/main.rs`: binary entry point; dispatches CLI, MCP server or TUI
- `src/cli/mod.rs`: CLI module root and dispatch
- `src/cli/args.rs`: clap command and argument definitions
- `src/cli/cmd.rs`: CLI command handlers
- `src/mcp/mod.rs`: MCP module root and server setup
- `src/mcp/params.rs`: tool parameter structs (serde + JSON Schema)
- `src/mcp/tools.rs`: MCP tool implementations and tests
- `src/tui/`: interactive ratatui terminal UI
- `src/service.rs`: business logic (create, update, moves, epic progress, auto-close, archive, store check)
- `src/store.rs`: core task storage and file parsing/writing
- `src/task.rs`: task model, frontmatter serde, hierarchical ID helpers and generation
- `src/scaffold.rs`: `bea init` / `bea agent` harness scaffolding (templates in `templates/`)
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
- `id`: hierarchical, lowercase alphanumeric segments separated by `.` (`a1b`, `a1b.c2`, `a1b.c2.d7`); the ID names the parent epic
- `title`
- `status`: `proposed | open | in_progress | review | done | blocked | cancelled`
- `priority`: `P0 | P1 | P2 | P3`
- `type`: `task | epic` (defaults to `task`, omitted from frontmatter when `task`)
- `created`, `updated` (timestamps)
- `tags`: list
- `depends_on`: list of task IDs
- `assignee`: string
- `attempts`: how many times work was started (omitted when zero)
- body: markdown content after frontmatter

Frontmatter parsing approach:
- Split on `---` delimiters
- Parse YAML with `serde-saphyr`, always through `task::from_yaml` / `task::to_yaml`, which fix the on-disk shape
- Preserve trailing markdown as body string

There is **no `parent` field**: a task's parent is derived from its ID (`Task::parent()`). Never add a `parent:` key, edit a task's `id`, or rename a `.bears/` file by hand. A `parent:` key makes the store *legacy*: every command and MCP tool then refuses to load it (`Error::LegacyStore`) until `bea migrate` converts it. Move a task with `bea update <id> --parent <epic-id|"">` / `update_task`'s `parent` instead.

## Behavior Requirements

### ID generation
- A root ID is `id-length` chars (`.bears.yml`, default 3); a child ID is its parent's ID plus a `child-id-length` segment (default 2, grows as an epic fills up).
- Check collisions against active and archived tasks and their ancestors; regenerate if needed.
- Moving a task (`update --parent`) or deleting/pruning an epic renames tasks through `store::rename_ids`, which rewrites every `depends_on`; the result carries a `renamed` map of old ID → new ID.

### Ready task logic
- `ready` includes tasks with `status == open`, `type == task`, and all dependencies in `done`.
- Epics are excluded from ready results (they are tracked separately).
- Sort by priority (`P0` first), then creation date.

### Epic behavior
- Epics group child tasks by ID: a task's parent is the epic its ID is under.
- `bea epics` / `list_epics` shows epics with progress (done/total children).
- When all children of an epic are resolved (done or cancelled), the epic is automatically marked as done; this cascades up through nested epics.
- `epic` filters (`list`/`ready`/`review --epic`, the MCP `epic` parameters) take an ID or prefix and match the epic's whole subtree.
- Deleting or pruning an epic detaches its surviving children to fresh root IDs.
- Epics are excluded from `ready` results.

### Dependency safety
- Validate `dep add` operations do not create cycles.
- Reject cycle-forming edges with a clear error.

## MCP Tool Surface

Keep these tools aligned with implementation and schemas:
- `list_ready(limit?, tag?, epic?)`
- `list_all_tasks(status?, priority?, tag?, epic?, limit?, active_only?, include_proposed?)`
- `list_epics()`
- `get_task(id)`
- `create_task(title, priority?, tags?, depends_on?, parent?, body?, type?)`
- `update_task(id, title?, status?, priority?, tags?, assignee?, body?, parent?)` (a new `parent` moves the task; result carries `renamed`)
- `start_task(id, assignee?)`
- `release_task(id)`
- `list_review(limit?, tag?, epic?)`
- `review_task(id)` / `reject_task(id)`
- `propose_task(id)` / `accept_task(id)`
- `complete_task(id)`
- `cancel_task(id)`
- `prune_tasks(include_done?)` (returns `{ pruned, renamed }`)
- `add_dependency(id, depends_on)`
- `remove_dependency(id, depends_on)`
- `delete_task(id)` (adds `renamed` when an epic's children were detached)
- `search_tasks(query, limit?, active_only?)`
- `plan_epic(id)`
- `get_graph(include_done?, epic?, limit?)`
- `archive_task(id?)` / `restore_task(id)` / `list_archived(limit?)`
- `doctor()` (read-only store check)

`bea migrate [--dry-run]` (convert a pre-0.10 store) and `bea doctor --fix` have no MCP equivalent; the MCP `doctor` tool only checks.

## Error Handling Expectations

- Missing `.bears/` should provide actionable guidance (suggest `bea init`).
- A legacy store (`parent:` keys) is refused at load time with `Error::LegacyStore`; the frontends suggest `bea migrate`.
- Unknown task IDs should produce clear errors.
- Cycle detection errors should explain why the edge is rejected.
- Invalid frontmatter should warn-and-skip problematic files, not crash the process.
- Library code should use `Result` + `?`; CLI formats user-facing errors; MCP methods map to `rmcp::ErrorData`.

## Dependencies

Use and keep dependency scope minimal:
- `clap`, `clap_complete`, `serde`, `serde-saphyr`, `serde_json`, `jiff`, `rand`, `thiserror`, `tokio`, `rmcp`, `schemars`, `owo-colors`

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
