# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

`bea-rs` is a file-based task tracker named `bears` (binary: `bea`). It manages a task/issue graph stored as markdown files with YAML frontmatter in a `.bears/` directory.

The crate ships **both a library and a binary**. The package is `bea-rs`; the library is `bears` (`[lib] name = "bears"`, since `bears` was taken on crates.io). The library is the core; the CLI, MCP server, and TUI are binary-only frontends built on top of it. It has three modes:

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
cargo build --lib --no-default-features   # Verify the library still stands alone
```

## Task tracking

This project uses `bears` itself to manage its own tasks. The bears MCP server is registered with Claude Code — use the MCP tools (`list_ready`, `create_task`, `complete_task`, etc.) to check and update tasks rather than running `bea` CLI commands directly.

## Development workflow

Work test-driven: write tests before or alongside implementation. After finishing any task, always run:

```bash
cargo fmt && cargo clippy && cargo test
```

All three must pass cleanly before considering a task done.

### CI

`.github/workflows/push.yml` runs on pushes and pull requests that touch Rust sources, `Cargo.toml`,
`Cargo.lock`, `.cargo/audit.toml` or the CI workflows themselves, weekly on a schedule (Monday 06:17
UTC), and on manual dispatch. It calls the reusable `quality-checks.yml`, which `release.yml` also
runs before building a tag. That workflow has two jobs:

- **Format, Lint, and Test** — `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`.
- **Dependency advisories** — `cargo audit` (pinned cargo-audit, installed with
  `taiki-e/install-action`) against `Cargo.lock`. `.cargo/audit.toml` sets `deny = ["warnings"]`, so
  unmaintained, unsound and yanked crates fail as well as vulnerabilities. An advisory may only be
  ignored there by its id, with a comment giving the reason; never add a blanket ignore. Run
  `cargo audit` locally after changing dependencies.

## Commit style

Use [Conventional Commits](https://www.conventionalcommits.org/): `type(scope): description`. Common types: `feat`, `fix`, `ci`, `docs`, `chore`, `refactor`, `test`. Release notes are generated from commit messages, so keep them accurate and descriptive.

## Architecture

```
src/
  lib.rs           # LIBRARY ROOT — public API surface (see "Library/binary split")
  main.rs          # `bea` BINARY entry point: dispatch to CLI, MCP server, or TUI
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
  service.rs       # Business logic: create, update, epic progress, auto-close, archive
  store.rs         # Core: parse .bears/ dir, read/write task files, archive layer
  task.rs          # Task struct, frontmatter serde, enums, ID generation
  graph.rs         # Dependency graph: build, ready, effective priority, cycle detection, dep tree
  scaffold.rs      # `bea init` harness-integration scaffolding (Claude/Copilot/Codex)
  config.rs        # .bears.yml configuration loading
  editor.rs        # $EDITOR integration for `bea edit` (binary-only)
  error.rs         # thiserror error types
templates/         # Embedded harness templates (via include_str!) for init scaffolding
```

### Library/binary split

`src/lib.rs` exposes `config`, `error`, `graph`, `scaffold`, `service`, `store`, `task`, plus the
`Error`/`Result`/`Task`/`Status`/`Priority`/`TaskType` re-exports. `cli`, `mcp`, `tui`, and `editor`
are declared in `main.rs` and are **not** part of the library.

Consequences to respect when editing:

- The core must never reference `cli`, `mcp`, or `tui`. Dependency arrows point inward only.
- Frontend code refers to the core as `bears::store`, **not** `crate::store` — the binary is a
  separate crate from the library.
- `pub(crate)` in a core module is invisible to the frontends. Anything a frontend needs must be `pub`.
- Frontend-only dependencies (clap, ratatui, crossterm, rmcp, notify, owo-colors, tui-markdown,
  shell-words, clap_complete) are **optional** and gated behind the default `cli` feature; `[[bin]]`
  carries `required-features = ["cli"]`. Adding a dep used only by a frontend means adding it to
  that feature list, not just `[dependencies]`.
- `schemars::JsonSchema` derives on the core enums are gated behind the `schema` feature via
  `#[cfg_attr(feature = "schema", derive(...))]`, because JSON Schema is an MCP concern.
- **Error messages carry no frontend hints.** The library states what went wrong; the frontend adds
  remediation. CLI hints live in `hint_for()` in `main.rs`.

Verify both configurations build after touching the split:

```bash
cargo build --lib --no-default-features   # library must stand alone
cargo build                               # binary with all frontends
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
id: x9k.a1
title: Implement OAuth flow
status: open          # proposed | open | in_progress | review | done | blocked | cancelled
priority: P1          # P0 (critical) through P3
type: task            # task (default) | epic
created: 2026-03-15T10:30:00Z
updated: 2026-03-15T10:30:00Z
tags: [backend, auth]
depends_on: [f4c9, b7e3]
assignee: alice
attempts: 2
---

Markdown body here.
```

Parse frontmatter by splitting on `---` delimiters, using `serde-saphyr` for the YAML portion, keeping the rest as the body string. All YAML goes through `task::from_yaml` / `task::to_yaml`, which fix the on-disk shape: fields in struct order, block lists indented under their key, timestamps double-quoted, strings a YAML 1.1 reader could take for a bool, number or null (`yes`, `nan`) double-quoted, long titles kept on one line, and parse errors on one line without a source snippet. String fields keep an unquoted scalar exactly as written (`title: 1.50` stays `1.50`), so files written by earlier versions — and hand edits — load unchanged. Optional fields (`tags`, `depends_on`, `assignee`, `attempts`) are skipped on write when empty, and default when absent on read.

There is no `parent` field: a task's parent is derived from its ID (`Task::parent()`; `x9k.a1` above is a child of epic `x9k`). Files written before hierarchical IDs carry a `parent:` key. It still parses, into `Task::legacy_parent`, which is never written back and has no effect on the parent; it exists so that legacy stores can be detected and migrated. Any write of such a task drops the key — which is why a legacy store is refused outright (next section).

### Legacy stores and `bea migrate`

A store is **legacy** when any task file in `.bears/` or `.bears/archive/` carries a `parent:` key.
`store::load_all` and `store::load_archived` both check *both* directories and return
`Error::LegacyStore { files }` instead of any tasks. The directory a loader parses is checked through
`legacy_parent`; the other one is only scanned for a `parent:` line at column 0 inside the frontmatter
(non-null value, no YAML parse), so a load never parses the directory it does not return. No write path can therefore run on a legacy store and
silently drop an epic link. This covers the CLI, every MCP tool and the TUI alike; only commands that
never load the store (`init`, `agent`, `completions`, and `migrate` itself) work on one. The CLI hint
pointing at `bea migrate` lives in `hint_for()`.

`store::migrate(base, dry_run) -> Migration { renamed, orphaned, legacy_files }` converts the store;
`bea migrate [--dry-run]` is the CLI on top (there is no MCP tool):

- Every task whose `parent:` names an epic (active or archived, by exact ID) gets a fresh child ID
  under that epic's *new* ID via `generate_child_id`, parents first, so nested epics carry the whole
  chain. Roots keep their IDs. A task found below a renamed task by its ID alone keeps its last
  segment under the new prefix.
- A `parent:` that names no task (`MissingParent`), a non-epic (`ParentNotEpic`) or leads round a
  cycle (`ParentCycle`; the cycle is broken at one task) is **dropped and reported** in `orphaned`;
  the task keeps its ID and is not lost.
- All `depends_on` are rewritten in active and archived tasks, `updated` is kept, files with nothing
  to change are not touched, and afterwards no file has a `parent:` key.
- `--dry-run` prints the same mapping and writes nothing. On a store without `parent:` keys it is a
  no-op ("Nothing to migrate", exit 0), so it is idempotent.
- **Interruption safety.** The renames are written to `.bears/.migrate.json` (`MIGRATION_JOURNAL`)
  before any task file; then new files, then rewritten files, then the old files are removed, and the
  journal last. While unfinished, old files and their `parent:` keys remain, so the store still
  refuses to load, and a re-run reuses the journal's IDs (recognising files already written under a
  new ID as copies, and an already-removed epic's new ID through the journal) to finish the job
  without duplicating or losing a task. It does not use `rename_ids`, which refuses a half-applied
  rename (`IdTaken`) and cannot resume.

CLI output: one `  renamed <old> → <new>` line per rename (as `bea update` prints), one
`  kept <id> as is: its parent <p> does not exist | is not an epic | is part of a parent cycle, so the
link was dropped` line per orphan, then a summary line. `--json` prints
`{ "dry_run", "renamed": {old: new}, "orphaned": [{ "id", "parent", "reason" }], "legacy_files" }`,
`reason` being `missing-parent`, `parent-not-epic` or `parent-cycle`. Exit status 0 on success
(including nothing to do), 1 on an error (e.g. `NotInitialized`).

### ID generation

Generate a short lowercase alphanumeric ID (configurable length via `.bears.yml`, default 3 chars). Check for collisions against both active and archived tasks and their ancestors; regenerate if needed.

**Hierarchical IDs.** An ID encodes its parent
chain: `a1b` root, `a1b.c2` child, `a1b.c2.d7` grandchild — segments separated by `.`, each
lowercase alphanumeric. `task.rs` holds the model as plain `&str` helpers: `validate_id` /
`is_valid_id`, `id_segments`, `id_depth` (0 for a root), `parent_of`, `root_of`, `leaf_segment`,
`is_descendant` (strict, whole segments), `id_from_filename` (`{id}-{slug}.md`; `-` never occurs in
an ID), and `generate_child_id(parent, existing, &Config)`. Child segments start at
`child-id-length` in `.bears.yml` (default 2, range 1–8, written only when not the default) and grow
by one char for new children once more than 25% (`CHILD_SPACE_FILL_LIMIT`) of the parent's sibling
space at the current length is used; existing IDs keep their length. `existing` is the collision
pool (active + archived IDs), and siblings are counted from it. `service::create_task` with a parent
resolves it (it must exist and be an epic, else `TaskNotFound` / `ParentNotEpic`) and generates the
new ID with `generate_child_id`; without one it uses `generate_id` with `id-length`. The collision
pool is `store::reserved_ids(base, &tasks)`: active and archived IDs plus every ancestor of each
(`task::with_ancestors` / `task::ancestors`). Ancestors count even when their task is gone, so a new
task can never take the ID of a deleted epic and silently adopt its orphaned children.

**Prefix resolution.** `store::resolve_prefix` (used for active and archived lookups alike, so CLI,
MCP and TUI agree) takes an exact ID first. Otherwise every ID starting with the prefix matches, and
when all matches are one task and its descendants it resolves to that shallowest task: `8p` is the
epic `8ph` even with `8ph.c2` present, and `8ph.c` (ending inside a child segment) is `8ph.c2` even
with `8ph.c2.d7` present. Matches that are genuinely different tasks — siblings, separate roots, or
the orphaned children of a deleted parent — are still `AmbiguousPrefix`. An empty or blank prefix
would match everything and is rejected with `InvalidId`; callers that give `""` a meaning (the
"no parent" of `create`/`update --parent ""`) handle it before resolving.

**Renaming.** `store::rename_ids(base, &BTreeMap<old, new>)` is the one primitive that changes IDs on
disk. Each key's task is rewritten under its new ID in the directory it lives in (active or archive),
and every `depends_on` naming a key is rewritten in every task, active and archived; `updated` is not
touched, and descendants are not implied (a subtree move lists every ID). It validates everything
first (`TaskNotFound` for an unknown key, `InvalidId`, `IdTaken` for a new ID shared by two keys or
already the ID of any task, active or archived), then writes the new files, then the rewritten references, and
removes the old files last, so an interruption can leave a task under both IDs but never loses one.
That guarantee rests on the precondition that old and new IDs are disjoint: swaps (`a → b, b → a`),
chains (`a → b, b → c`) and identity entries are refused with `IdTaken`, since renaming onto an ID
still being renamed away could overwrite its file before its own new file exists. Callers rename to
fresh IDs.
Reparenting is built on it; `bea doctor --fix` and detaching an epic's children on delete are meant
to be too. `bea migrate` is not: it needs to resume a half-applied rename (see below).

### `ready` command

The key command for agent workflows. Returns tasks where status is `open`, type is `task` (not `epic`), AND all `depends_on` tasks have status `done`. Sort by priority (P0 first), then creation date. `proposed` and `review` tasks are excluded for free by the `status == Open` check — see the review workflow section.

### Review & proposal workflow

The normal path is `open → in_progress → review → done`, with two statuses either side of the work:

- **`proposed`** — suggested, not yet accepted into the backlog. Never ready, and filtered out of
  `list_tasks` unless the caller passes `include_proposed` or filters `status = proposed`
  explicitly (`bea list --all` passes both flags; MCP's `list_all_tasks` takes `include_proposed?`).
- **`review`** — finished work waiting for a second pair of eyes. Never ready, and because
  `is_task_ready` requires dependencies to be `done`, a task in review does **not** unblock its
  dependents. Epics do not auto-close on it either, since auto-close needs every child resolved.

`service::list_review` is the review queue, deliberately *not* folded into `list_ready`: reviewing
someone else's work and starting fresh work are different jobs, so an orchestrator can dispatch them
to different workers. `bea review` doubles as both verbs — with an ID it submits, without one it
lists the queue (the same shape as `bea archive [id]`).

**Workflow verbs are single edges.** `service::transition` is the shared helper: each verb accepts
exactly one starting status and returns `Error::InvalidStatus { action, expected, actual }`
otherwise, so a shortcut can never quietly undo unrelated state.

| Verb | Edge |
|------|------|
| `review` | `in_progress` → `review` |
| `reject` | `review` → `open` (assignee kept — rework routes back to the author) |
| `propose` | `open` → `proposed` |
| `accept` | `proposed` → `open` |
| `release` | `in_progress` → `open` (assignee cleared) |

`start` / `done` / `cancel` predate this and stay permissive blunt status setters, as does
`bea status <id> <status>` / `update_task` — that pair is the documented escape hatch for any move
the verbs refuse.

### Attempts

`attempts: Option<u32>` counts how many times work has been *started* on a task. Absent means zero —
`Task::attempt_count()` is the accessor that normalises that, and the field is only written once a
task has actually been started. It rides along in `TaskSummary`, but only when non-zero.

`service::record_attempt` bumps the counter on every transition **into** `in_progress` — that covers
`start_task`, `set_status`, and the `update_task` status path, so `bea start`, `bea status`,
`bea update --status`, the TUI and every MCP equivalent all agree. Restarting a task that is already
`in_progress` (e.g. handing it to another assignee) is the *same* attempt and does not count.

Counting at claim time rather than at release time is deliberate: the counter is already durable if a
worker dies without releasing, the worker can read "this is attempt 3" when it picks the task up, and
a first-try success honestly reads `attempts: 1`. `release_task` and `complete_task` never touch it.

### Assignee & task claiming

`assignee` is a plain string on `Task` (empty = unassigned) that rides along in the frontmatter and
in **`TaskSummary`** (skipped when empty), so every list/ready/start response tells an orchestrator
who holds a task without a second lookup.

| Operation | Effect on assignee |
|-----------|--------------------|
| `bea start <id> [-a NAME]` / `start_task { id, assignee? }` | `Some(name)` sets it, `Some("")` clears it, `None` (flag omitted) leaves it unchanged |
| `bea release <id>` / `release_task { id }` | cleared, and status goes back to `open` (`attempts` untouched) |
| `bea update <id> --assignee NAME` / `update_task` | set directly, independent of status |

`service::release_task` is deliberately narrow: it **only** accepts tasks whose status is
`in_progress`, returning `Error::NotInProgress` otherwise, so a released task can never silently
undo a `done`/`cancelled` state. The motivating flow is a stuck or killed worker agent — the
orchestrator releases the claim and the task reappears in `ready` for the next worker.

### Epic behavior

Epics group child tasks by ID: a task's parent is the epic its ID is under (`Task::parent()`, `Task::is_child_of()`), set when the task is created with a parent. Changing it later with `update --parent <id|"">` / `update_task`'s `parent` *moves* the task: it gets a fresh ID (a root ID with `id-length` for `""`, a child ID of the new epic otherwise), every task below it — archived ones included, in the archive — keeps its own segments under the new prefix, and every `depends_on` naming one of them is rewritten (`store::rename_ids`). The new parent must be an existing epic (`TaskNotFound` / `ParentNotEpic`) outside the task's own subtree (`ParentInSubtree`), checked before anything is written; naming the current parent is a no-op. `service::update_task` returns `UpdatedTask { task, renamed }`, `renamed` being the old → new map (empty when nothing moved); the CLI prints it and adds it to `--json` as `renamed`, and the MCP tool adds the same `renamed` key. The field edits are saved under the old ID before the rename (if the rename then fails, they stay saved). A move by itself never opens or closes an epic: auto-close stays tied to status changes, so neither the old nor the new parent is re-checked unless the same update changes the status, in which case the check runs at the new parent. Children counts, `epic` filters and auto-close use direct children only. Deleting or pruning an epic leaves its children's IDs, and so their parent, as they are. `bea epics` / `list_epics` shows epics with progress (done / total non-cancelled children). An epic auto-closes when every child is *resolved* — i.e. `done` **or** `cancelled` (cancelled children are non-blocking and excluded from the total). Auto-close runs on both the `set_status` and `update_task` status paths and cascades up through nested epics. Epics are excluded from `ready` results.

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
| `list_all_tasks` | `status?`, `priority?`, `tag?`, `epic?`, `limit?`, `active_only?`, `include_proposed?` |
| `list_epics` | — |
| `get_task` | `id` |
| `create_task` | `title`, `priority?`, `tags?`, `depends_on?`, `parent?`, `body?`, `type?` |
| `update_task` | `id`, `title?`, `status?`, `priority?`, `tags?`, `assignee?`, `body?`, `parent?` |
| `start_task` | `id`, `assignee?` |
| `list_review` | `limit?`, `tag?`, `epic?` |
| `review_task` | `id` |
| `reject_task` | `id` |
| `propose_task` | `id` |
| `accept_task` | `id` |
| `release_task` | `id` |
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

On `update_task`, a `parent` other than the current one (`""` for none) moves the task and its subtree to new IDs; the result then carries `renamed: { old: new }`. `get_task`'s `parent` is derived from the ID. `active_only?` (on `list_all_tasks` / `search_tasks`) excludes done/cancelled tasks. `get_task` resolves active tasks first and **falls back to the archive** when the id isn't active, marking the returned detail with `"archived": true` (read-only — restore before mutating).

## Dependencies

- `clap` (derive) — CLI parsing
- `clap_complete` — shell completion generation
- `serde`, `serde-saphyr`, `serde_json` — serialization (`serde-saphyr` is pure safe Rust; it replaced `serde_yml`, which is unsound and unmaintained per RUSTSEC-2025-0068)
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
- `libc` (unix only, binary-only) — reset `SIGPIPE` to its default disposition for CLI output

Keep the dependency tree small. Compilation should be fast.

## Error handling

- `.bears/` not found → suggest `bea init`
- Legacy store (`parent:` keys) → `Error::LegacyStore` at load time, CLI hint suggests `bea migrate`
- Task ID not found → clear error
- Cycle detected → reject with explanation
- Invalid frontmatter → warn and skip (don't crash), report which file
- Library code uses `Result` with `?`. CLI formats errors for humans. MCP returns `rmcp::ErrorData` from tool methods.
- **Broken pipes.** Rust ignores `SIGPIPE` at startup, so `println!` to a closed pipe (`bea show | head`)
  panics instead of exiting. `restore_sigpipe()` in `main.rs` puts the default disposition back before
  the CLI runs — the process then dies from the signal like any other Unix filter. It is **not** applied
  to `bea mcp` or `bea tui`, which own their transports. This is why the ~70 `println!` calls in
  `cli/cmd.rs` can stay as they are; there is a regression test in `tests/cli.rs`
  (`test_broken_pipe_exits_quietly`, unix-only). Windows has no `SIGPIPE` and is not covered.

## Testing

- Unit tests in `graph.rs`: cycle detection, topological sort, ready computation
- Unit tests in `task.rs`/`store.rs`: frontmatter parsing (valid, missing fields, extra fields, malformed); the exact on-disk frontmatter shape, and a render/parse round-trip over every file in this repository's own `.bears/` and `.bears/archive/` (unchanged except that a legacy `parent:` key is dropped); archive storage layer (`move_to_archive`, `move_from_archive`, `load_archived`, `all_known_ids`); `rename_ids` (active + archive, references rewritten, `updated` kept, untouched files left alone, swap/chain/identity maps refused, validation before any write); legacy stores (both loaders refuse a `parent:` key in either directory; the cheap scan ignores `parent:` text in a body, block scalar or indented value) and `migrate` (nested epics, archived child of an active epic and the reverse, references and `updated`, missing/non-epic/cyclic parents kept as roots, dry run writes nothing, second run a no-op, resume from every interruption point incl. an already-removed epic, and a migrated copy of this repository's own `.bears/` loading with every former parent link as an ID prefix)
- Unit tests in `task.rs`/`store.rs` for hierarchical IDs: validation (flat and dotted, malformed rejected), segments/depth/`parent_of`/`root_of`/`leaf_segment`/`is_descendant`, `id_from_filename`, child-segment growth threshold (only direct children of that length count; lengths coexist after growth), collision avoidance, skipping YAML-unsafe IDs, dotted-ID round-trip, and dotted IDs in `find_task_path`/`resolve_prefix`/`archived_id_set`
- Unit tests in `config.rs`: `id-length` and `child-id-length` defaults, loading, bounds validation, and serialisation (`child-id-length` written only when not the default)
- Unit tests in `task.rs`: parent derived from the ID, no `parent:` key written, legacy `parent:` key read into `legacy_parent` but never written or used as the parent
- Unit tests in `service.rs`: epic progress, auto-close (incl. cancelled children and nested cascade); child creation (hierarchical ID under the resolved parent, nested epics, `child-id-length`, archived-ID avoidance, ancestors of orphans reserved at root and child level); reparenting (subtree moved with archived descendants and active/archived dependants rewritten, detach to a fresh root ID, root task moved under an epic, auto-close at the new parent, self/descendant/non-epic/unknown parents refused before any write), unchanged parent accepted; start/release (assignee set/keep/clear, release rejects non-`in_progress`); attempts (counted on every path into `in_progress`, not on restart/release/other transitions); review/proposal workflow (edge preconditions, review not unblocking dependents or closing epics, proposals hidden from listings); archive service (`is_archivable`, `archive_task`, `archive_all`, `restore_task`, `list_archive`, `get_archived_task`, archived-ID collision avoidance)
- Unit tests in `mcp/tools.rs`: tool create/list/start/complete/search/graph/plan_epic/delete/deps/validation; derived parent and reparenting (`renamed` map, detach, refusal under its own subtree); archive tools (archive/restore/list_archived); end-to-end archive visibility (hidden from list/search/graph/epics), integrity (dep add onto archived ID, prune never touches archive, no ID reuse)
- Unit tests in `graph.rs`: effective-priority correctness, DAG dep-tree bounding, bounded adjacency, plus `#[ignore]`d coupled-graph perf benchmarks (run with `cargo test -- --ignored`)
- Unit tests in `scaffold.rs`: `.mcp.json` merge (preserve/idempotent/fresh) and harness scaffolding (claude/copilot/codex skill/agent files, `bea mcp` binary form)
- Unit tests in `tui/`: input truncation (UTF-8 safety), ready-filter parity, watcher (the watcher test is `#[ignore]`d as timing-sensitive)
- Integration tests in `tests/cli.rs`: create a temp `.bears/` dir, run commands, verify file output; hierarchical child IDs and `update --parent` moves (renamed output and `--json` map, archived dependant rewritten, detach, no-op, self-parent refused); legacy store refused by every loading command with the `bea migrate` hint and nothing written; `bea migrate` (`--dry-run`, `--json` map, orphan report, idempotent re-run, not initialized); archive/restore/log/list-archived e2e; `bea init --claude/--copilot/--codex` scaffolding e2e (file presence, idempotency, MCP merge, `bea mcp` binary form, template packaging guard)
- E2E tests in `tests/mcp.rs`: spawn the real `bea mcp` binary and drive it over stdio with the
  rmcp client (dev-dependency features `client`, `transport-child-process`) — handshake, tool
  list/schemas, tool calls, protocol- vs tool-level error semantics, and a legacy store refused as a
  tool error
- MCP tools also verified end-to-end via live MCP sessions
