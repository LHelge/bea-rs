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

`.github/workflows/snapshot.yml` dogfoods `bea snapshot` on this repository's own `.bears/`. It runs
on pushes to `main` and on pull requests that touch `.bears/**`, `src/**`, `templates/snapshot/**`,
`Cargo.toml`, `Cargo.lock` or the workflow (one path list, written out under both triggers: the
output depends on the core modules and the embedded HTML template as well as the store, and `main`'s
snapshot is re-rendered after a renderer change is merged), and on manual dispatch. It builds `bea` from the
checkout (the released binary may predate the renderers being changed), appends
`bea snapshot --format md` to `$GITHUB_STEP_SUMMARY`, and uploads
`bea snapshot --all --format html` as the `bears-snapshot` artifact. It is deliberately separate from
`quality-checks.yml`, so a snapshot problem never blocks the quality gate or a release, and runs with
`permissions: contents: read`, no secrets and no PR comments, on `pull_request` (never
`pull_request_target`), so it is safe for forks. The README's user-facing example installs a released
`bea` instead; keep the two in step when the snapshot flags change.

**Release notes.** `release.yml` writes the GitHub release body with `git-cliff --latest`, configured
by `cliff.toml` at the repository root (not packaged), with git-cliff pinned to the version the
template was checked with (`git-cliff@2.14.2`, like cargo-audit; preview the notes before raising
the pin). Its release job checks out with
`fetch-depth: 0`, since git-cliff needs the history and tags back to the previous release. The
notes start with a "Breaking changes" section: every breaking commit (`type!:` or a
`BREAKING CHANGE:` footer), each followed by its footer's text, the upgrade step, as an indented
paragraph. Breaking commits are listed there only, not again under their type; the other commits
follow grouped by type (Features, Bug Fixes, Documentation, …), and `protect_breaking_commits`
keeps a breaking commit even where a parser would skip it (`chore(deps)`). The footer is taken
from the raw message, from `BREAKING CHANGE:` to the next blank line, because git-cliff's own
footer parser cuts it at any `word:` inside (`Error::Yaml`). Preview the notes of the next
release with `git-cliff --unreleased --tag vX.Y.Z` (or `git-cliff vA..vB` for a past range).

## Commit style

Use [Conventional Commits](https://www.conventionalcommits.org/): `type(scope): description`. Common types: `feat`, `fix`, `ci`, `docs`, `chore`, `refactor`, `test`. Release notes are generated from commit messages, so keep them accurate and descriptive. Mark a breaking change with `!` and give it a `BREAKING CHANGE:` footer that tells the user what to do; the footer is printed under the change in the release notes. End the footer with a blank line before any trailers (`Co-Authored-By:` …), or the trailers become part of it.

## Architecture

```
src/
  lib.rs           # LIBRARY ROOT — public API surface (see "Library/binary split")
  main.rs          # `bea` BINARY entry point: dispatch to CLI, MCP server, or TUI
  cli/
    mod.rs         # CLI module root and dispatch
    args.rs        # clap command and argument definitions
    cmd.rs         # Command handlers (list, show, create, edit, graph, snapshot, etc.)
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
  snapshot/        # `bea snapshot` data model: deterministic, serialisable store picture
    mod.rs         # Snapshot, SnapshotOptions, build/load, layered dependency graph
    tests.rs       # Unit tests for the data model
    markdown.rs    # GitHub-flavored Markdown renderer with a Mermaid dependency graph
    markdown/tests.rs  # Unit tests for the Markdown renderer
    html.rs        # Self-contained HTML page with an SVG dependency graph (render)
    html/tests.rs  # Unit tests for the HTML renderer
  store.rs         # Core: parse .bears/ dir, read/write task files, archive layer
  task.rs          # Task struct, frontmatter serde, enums, ID generation
  graph.rs         # Dependency graph: build, ready, effective priority, cycle detection, dep tree
  scaffold.rs      # `bea init` harness-integration scaffolding (Claude/Copilot/Codex)
  config.rs        # .bears.yml configuration loading
  editor.rs        # $EDITOR integration for `bea edit` (binary-only)
  error.rs         # thiserror error types
templates/         # Embedded templates (via include_str!): harness files for init scaffolding,
                   # and snapshot/snapshot.html, the page of `bea snapshot --format html`
```

### Library/binary split

`src/lib.rs` exposes `config`, `error`, `graph`, `scaffold`, `service`, `snapshot`, `store`, `task`, plus the
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
- `--json` flag on all CLI commands outputs JSON instead of human text (except `bea snapshot`, whose
  output is a rendered artifact; it refuses `--json` with `InvalidUsage`).
- MCP tools return minimal structured data (id, title, priority, status, tags) — not full markdown bodies.
- Frontends never assume an ID's length: dotted IDs (`a1b.c2.d7`) vary, so CLI list output (`list`,
  `ready`, `review`, `stale`, `prime`, `epics`, `search`, `list --archived`, `log`) and the TUI task list pad each ID
  to the longest one shown (`id_width` / `id_cell` in `cli/mod.rs`) so the next column lines up.
  Lists sort by priority then creation date, with the ID only as the last tie-break
  (`task::canonical_order`; `Graph::ready` uses effective priority), so exact ties never follow
  the store map's iteration order; trees (`graph`, `dep tree`, the TUI
  subtask tree) follow the dependency or parent structure.
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

Tasks are stored as `.bears/{id}-{slug}.md` with YAML frontmatter. The slug is the title lowercased with every non-ASCII-alphanumeric run turned into one dash, cut to `task::MAX_SLUG_LEN` (100) bytes so a long title cannot exceed the 255-byte filename limit; a file is found by its ID, never its slug, so one written with a longer slug still loads and is renamed by its next write:

```markdown
---
id: x9k.a1
title: Implement OAuth flow
status: open          # proposed | open | in_progress | review | done | blocked | cancelled
priority: P1          # P0 (critical) through P3
# type: epic          # only written for epics; absent means task
created: "2026-03-15T10:30:00Z"
updated: "2026-03-15T10:30:00Z"
tags:
  - backend
  - auth
depends_on:
  - f4c
  - x9k.b7
assignee: alice
attempts: 2
---

Markdown body here.
```

Parse frontmatter by splitting on `---` delimiters, using `serde-saphyr` for the YAML portion, keeping the rest as the body string. All YAML goes through `task::from_yaml` / `task::to_yaml`, which fix the on-disk shape: fields in struct order, block lists indented under their key, timestamps double-quoted in the form of `task::timestamp` (RFC 3339, UTC with `Z`, and no, 3, 6 or 9 fractional digits, whichever is the shortest exact one — what `chrono` wrote, kept so that rewriting a file never changes its timestamps; any offset or precision is read), strings a YAML 1.1 reader could take for a bool, number or null (`yes`, `nan`) double-quoted, long titles kept on one line, and parse errors on one line without a source snippet. A parse error's text is that of `Error::Yaml`'s `Display` (`yaml_message` in `error.rs`), worded for whoever edits the file: what is wrong and `at line L, column C` (counted from the first line after `---`), never the YAML library's advice to its Rust caller (`use Option<String>`, `set DuplicateKeyPolicy in Options`, `invalid u32`). The cases a hand edit is likely to hit are reworded by the library's error variant and its fields, never by matching its message text (``expected a value, found null``, ``duplicate key `title` ``, ``expected a list``, ``unknown value `started`, expected one of …``, ``expected a whole number from 0 to 4294967295`` (the field type's range, so it is true of a number too large as well), ``expected a single value, not a list or mapping`` (also for a list or mapping given for `status`, `priority` or `type`), ``invalid timestamp `yesterday`, expected a date and time such as `2026-03-15T10:30:00Z` ``); anything else goes to the library's `UserMessageFormatter`, so an unknown error still says what went wrong. The library error is not the `source()`, so walking the chain cannot bring the advice back. Load warnings, `bea doctor`'s `unreadable-file`, `bea edit`, `.bears.yml` and MCP all show this text. String fields keep an unquoted scalar exactly as written (`title: 1.50` stays `1.50`), so files written by earlier versions — and hand edits — load unchanged. Optional fields (`tags`, `depends_on`, `assignee`, `attempts`) are skipped on write when empty, and default when absent on read; so is `type` when it is `task`.

There is no `parent` field: a task's parent is derived from its ID (`Task::parent()`; `x9k.a1` above is a child of epic `x9k`). Files written before hierarchical IDs carry a `parent:` key. It still parses, into `Task::legacy_parent`, which is never written back and has no effect on the parent; it exists so that legacy stores can be detected and migrated. Any write of such a task drops the key — which is why a legacy store is refused outright (next section).

### Legacy stores and `bea migrate`

A store is **legacy** when any task file in `.bears/` or `.bears/archive/` carries a `parent:` key.
`store::load_all` and `store::load_archived` both check *both* directories and return
`Error::LegacyStore { files }` instead of any tasks. The directory a loader parses is checked through
`legacy_parent`; in the other one only files whose frontmatter contains the word `parent` anywhere
(indented frontmatter, quoted and complex keys included) are parsed, to confirm the key, so a load does
not parse the directory it does not return, and both loaders agree on every file (one that does not
parse is never legacy) except a quoted key spelled with escapes (`"p\u0061rent"`), which no bears
version writes. No write path can therefore run on a legacy store and
silently drop an epic link. This covers the CLI, every MCP tool and the TUI alike; only commands that
never load the store (`init`, `agent`, `completions`, and `migrate` itself) work on one. The CLI hint
pointing at `bea migrate` lives in `hint_for()` in `main.rs`; the MCP frontend appends its own
remediation (run `bea migrate` once from a shell) to the tool error in `mcp/mod.rs` (`tool_ok` /
`hint_for`), and the server instructions name `bea migrate` too.

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
- **Deterministic IDs.** Unlike `create` and reparenting, the migration does not draw segments at
  random: `task::derive_child_id(parent, seed, existing, &Config)` takes candidate `0, 1, 2, …` of a
  fixed hash of the task's old ID (FNV-1a + SplitMix64, written out in `task.rs` so neither `std`'s
  hasher nor `rand` can change it) and keeps the first that is free and YAML-safe, with the same
  `child-id-length` / growth rule as `generate_child_id`. Tasks are placed in order of their old
  IDs (parents first) against a collision pool built from the files' IDs, so the result depends on
  nothing but the store's contents and `.bears.yml` — not the time, `updated` or directory order.
  The same legacy store therefore always migrates to the same IDs: two copies migrated on two
  branches get identical files, and a pinned test in `task.rs` guards the hash.
- `--dry-run` writes nothing and shows exactly the renames, final IDs included, that the real run
  then makes. On a store without `parent:` keys it is a no-op ("Nothing to migrate", exit 0), so it
  is idempotent.
- **Interruption safety.** The renames are written to `.bears/.migrate.json` (`MIGRATION_JOURNAL`)
  before any task file; then new files, then rewritten files, then the old files are removed, and the
  journal last. While unfinished, old files and their `parent:` keys remain, so the store still
  refuses to load, and a re-run reuses the journal's IDs (recognising files already written under a
  new ID as copies, and an already-removed epic's new ID through the journal) to finish the job
  without duplicating or losing a task. The journal wins over recomputation: an ID it records is
  kept even where `derive_child_id` would now pick another. It does not use `rename_ids`, which refuses a half-applied
  rename (`IdTaken`) and cannot resume.

CLI output: one `  renamed <old> → <new>` line per rename (as `bea update` prints), one
`  kept <id> as is: its parent <p> does not exist | is not an epic | is part of a parent cycle, so the
link was dropped` line per orphan, then a summary line. `--json` prints
`{ "dry_run", "renamed": {old: new}, "orphaned": [{ "id", "parent", "reason" }], "legacy_files" }`,
`reason` being `missing-parent`, `parent-not-epic` or `parent-cycle`. Exit status 0 on success
(including nothing to do), 1 on an error (e.g. `NotInitialized`).

### ID generation

Generate a short lowercase alphanumeric ID: a root ID of `id-length` chars (`.bears.yml`, default 3, range 2–8), a child ID of its parent's ID plus a `child-id-length` segment (below). Check for collisions against both active and archived tasks and their ancestors; regenerate if needed.

**Hierarchical IDs.** An ID encodes its parent
chain: `a1b` root, `a1b.c2` child, `a1b.c2.d7` grandchild — segments separated by `.`, each
lowercase alphanumeric. `task.rs` holds the model as plain `&str` helpers: `validate_id` /
`is_valid_id`, `id_segments`, `id_depth` (0 for a root), `parent_of`, `root_of`, `leaf_segment`,
`is_descendant` (strict, whole segments), `id_from_filename` (`{id}-{slug}.md`; `-` never occurs in
an ID), and `generate_child_id(parent, existing, &Config)`. Child segments start at
`child-id-length` in `.bears.yml` (default 2, range 1–8; always written, like `id-length`, and defaulted when absent) and grow
by one char for new children once more than 25% (`CHILD_SPACE_FILL_LIMIT`) of the parent's sibling
space at the current length is used; existing IDs keep their length. `derive_child_id(parent, seed, existing, &Config)` is the deterministic
counterpart (same rules, segments derived from `seed`), used only by `bea migrate`. `existing` is the collision
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

The `epic` filters resolve their value too, through `service::resolve_epic_filter(base, &tasks,
value)`, which the CLI (`list`/`ready`/`review --epic`) and the MCP `epic` parameters call before
filtering; the `list_*` functions and `bounded_adjacency_list` then take the resolved ID exactly.
The value is resolved against the active tasks first, as `show` and the mutating commands do, so an
archived task sharing the prefix never makes it ambiguous; an epic found there is used. When the
active tasks name nothing, several tasks or a non-epic, it is resolved again against active and
archived IDs together, so an epic that lives only in the archive still filters its active children
(several of which are ambiguous among the active tasks alone) and its ID or prefix names it.
Errors: `InvalidId` for an empty value; `TaskNotFound` when it names nothing, or a whole ID that no
task has while every match lies below it (`zz9` for orphans `zz9.a`, `zz9.b`); `AmbiguousPrefix`
when it names genuinely different tasks, active and archived together; `NotAnEpic` for a task that
is not an epic — an error, not the empty list these values gave before.

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
Reparenting and detaching an epic's children on delete/prune are built on it; `bea doctor --fix` is
meant to be too. `bea migrate` is not: it needs to resume a half-applied rename (see below).

### `ready` command

The key command for agent workflows. Returns tasks where status is `open`, type is `task` (not `epic`), AND all `depends_on` tasks have status `done`. Sort by effective priority (P0 first), then creation date, then ID for exact ties (so `--limit` cuts a deterministic list). `proposed` and `review` tasks are excluded for free by the `status == Open` check — see the review workflow section.

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
| `reject` | `review` → `open` (assignee cleared — the rework goes back to the pool) |
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
| `bea reject <id>` / `reject_task { id }` | cleared, and status goes from `review` back to `open` |
| `bea update <id> --assignee NAME` / `update_task` | set directly, independent of status |

`service::release_task` is deliberately narrow: it **only** accepts tasks whose status is
`in_progress`, returning `Error::NotInProgress` otherwise, so a released task can never silently
undo a `done`/`cancelled` state. The motivating flow is a stuck or killed worker agent — the
orchestrator releases the claim and the task reappears in `ready` for the next worker.

### Stale claims

`service::list_stale(&tasks, now, older_than: jiff::SignedDuration) -> Vec<StaleTask { task, idle }>`
returns the `in_progress` tasks whose `updated` is at least `older_than` before `now`, in canonical
order — the claims a dead worker may have left, for an orchestrator to `release`. `now` is a
parameter (the frontends pass `Timestamp::now()`), so the function is deterministic; a task updated
after `now` (clock skew) is never stale. `StaleTask::summary(eff)` is the JSON shape: the
`TaskSummary` plus `updated` and `idle_hours` (whole hours, rounded down). The default threshold is
`service::DEFAULT_STALE_HOURS` = 24. `bea stale [--hours N]` (`--json`) and the MCP `list_stale`
tool (`hours?`) are the frontends. It only lists — nothing is released — and there is deliberately
no heartbeat or lease: `updated` is the only signal, and any write to the task resets it.

### Session-start context (`bea prime`)

`service::prime(&tasks, ready_limit) -> Prime { ready, ready_total, in_progress, review }` gathers
what a coding agent needs when a session starts: the first `ready_limit` tasks of `list_ready` (and
`ready_total`, the whole queue's length; `Prime::ready_more()` is the difference), every
`in_progress` task (`list_tasks(Some(InProgress))`) and `list_review` — the same three listings as
`snapshot::build`. `Prime::summary(&eff) -> PrimeSummary` is the JSON shape. The default limit is
`service::DEFAULT_PRIME_READY` = 10. `bea prime [--limit N]` (`cmd_prime` in `cli/cmd.rs`) prints
the three sections with one ID width across them (in progress with assignee, or `unassigned`, and
`attempt N`; ready and review rows with `N earlier attempt(s)`, review also with the assignee), then a fixed reminder of the workflow verbs (`PRIME_WORKFLOW`),
colourless when stdout is not a terminal; `--json` prints the `PrimeSummary` (no reminder). There is
deliberately no MCP tool (an MCP client has `list_ready` / `list_all_tasks` / `list_review`). On a
directory without `.bears/` it fails with `NotInitialized` like every other command; the documented
Claude Code `SessionStart` hook is written `test ! -d .bears || bea prime`, so a project without a
store is no error. `bea init --claude` does not scaffold the hook (`.claude/settings.json` is not
merged); it is documented in the README and the Claude instructions template.

### Epic behavior

Epics group child tasks by ID: a task's parent is the epic its ID is under (`Task::parent()`, `Task::is_child_of()`), set when the task is created with a parent. Changing it later with `update --parent <id|"">` / `update_task`'s `parent` *moves* the task: it gets a fresh ID (a root ID with `id-length` for `""`, a child ID of the new epic otherwise), every task below it — archived ones included, in the archive — keeps its own segments under the new prefix, and every `depends_on` naming one of them is rewritten (`store::rename_ids`). The new parent must be an existing epic (`TaskNotFound` / `ParentNotEpic`) outside the task's own subtree (`ParentInSubtree`), checked before anything is written; naming the current parent is a no-op. `service::update_task` returns `UpdatedTask { task, renamed }`, `renamed` being the old → new map (empty when nothing moved); the CLI prints it and adds it to `--json` as `renamed`, and the MCP tool adds the same `renamed` key. The field edits are saved under the old ID before the rename (if the rename then fails, they stay saved). A move by itself never opens or closes an epic: auto-close stays tied to status changes, so neither the old nor the new parent is re-checked unless the same update changes the status, in which case the check runs at the new parent. Children counts, auto-close and the archive cascade use direct children only: a nested epic is one unit of work for its parent (auto-close cascades up on its own). The `epic` filters — `bea list`/`ready`/`review --epic` and the MCP `epic` parameter of `list_ready`, `list_all_tasks`, `list_review` and `get_graph` — match the epic's whole subtree instead (`task::in_epic`, built on `is_descendant`, whole segments), so work under a nested epic is found from the outer one; the epic itself is not in a list result (`get_graph` keeps it as a node). Deleting or pruning an epic first *detaches* its surviving direct children, active and archived: each gets a fresh root ID and its subtree moves with it, `depends_on` rewritten everywhere, exactly like `update --parent ""` (this restores the pre-0.10 behaviour, where delete/prune cleared the children's `parent`). The renames run first and the tasks are deleted last, deepest first, so an interruption never leaves a child under a parent that is gone; a task pruned inside a detached subtree is moved with it and then deleted under its new ID, and only survivors appear in the map. Detaching never opens or closes an epic. `service::delete_task` returns `DeletedTask { task, renamed }` and `service::prune_tasks` returns `PrunedTasks { tasks, renamed }` (deleted tasks under their original IDs; `renamed` holds survivors only, empty when nothing moved). The CLI prints `  renamed <old> → <new>` lines; the TUI shows the same pairs, by old ID, in a scrollable modal after a delete (`Mode::ShowRenames`, closed with Esc/Enter/q; not shown when nothing moved); `bea delete --json` / MCP `delete_task` add a `renamed` key to the summary, and `bea prune --json` / MCP `prune_tasks` always return `{ "pruned": [summaries], "renamed": {old: new} }`, `renamed` an empty object when nothing moved and `pruned` an empty array when nothing was pruned. `bea epics` / `list_epics` shows epics with progress (done / total non-cancelled children). An epic auto-closes when every child is *resolved* — i.e. `done` **or** `cancelled` (cancelled children are non-blocking and excluded from the total). Auto-close runs on both the `set_status` and `update_task` status paths and cascades up through nested epics. Epics are excluded from `ready` results.

### Snapshot data model

`snapshot::build(&active, &archived, &SnapshotOptions) -> Snapshot` (and `snapshot::load(base, &options)`,
which reads both maps first) is the data behind `bea snapshot`: pure, `Serialize`, no rendering. The
renderers (`snapshot/markdown.rs`, `snapshot/html.rs`) consume a `Snapshot` and never reload the store.

- **Agrees with the CLI.** `ready`, `review` and `in_progress` come from `service::list_ready`,
  `list_review` and `list_tasks(Some(InProgress))`; epic `progress` from `service::epic_progress`;
  `archive` from `service::archive_log` (the order of `bea log`).
- **Deterministic.** Every list has a fixed order with ties broken by ID, and the only time in it is
  `SnapshotOptions::generated_at`, passed in by the caller (`None` leaves it out).
- **`include_all`** (default off) adds settled detail: the `done`/`cancelled` board columns and graph
  nodes, done/cancelled epics, and the archive log (`archive_limit` entries, default 10). The counts
  (`totals`, `by_status`, `outstanding_by_priority`) always cover the whole store and never count epics.
- `epics` are in tree order with `parent`/`depth` (the nearest *shown* enclosing epic); every
  `SnapshotTask::epic` names the innermost shown epic, for subgraphs/groups.
- `graph`: nodes are the board's tasks plus shown epics that are an end of a dependency; edges run
  `from` dependency `to` dependent, only between nodes. Layers are longest-path over the DAG;
  dependency cycles (`Graph::cycles`) are condensed into one layer and marked `cyclic`. Each layer is
  ordered by the mean position of its dependencies, then epic, priority, creation date, ID.

**Markdown renderer.** `snapshot::markdown::render(&Snapshot) -> String` (and `render_with(&Snapshot,
&MarkdownOptions)`) is pure and deterministic, for GitHub job summaries and PR comments: summary
tables, epics with progress (nesting indented), the ready / in-progress / review queues, the board as
one `<details>` per non-empty column, a ```` ```mermaid ```` `flowchart LR` of `graph`, and the archive
log only with `include_all`.

- **Mermaid IDs are generated**, never task IDs (which contain dots): node `n{i}` is `graph.nodes[i]`,
  subgraph `e{i}` is `epics[i]`, so an epic that is also a node gets two distinct IDs and a node keeps
  its ID in every diagram. Each node sits in the subgraph of `SnapshotTask::epic`, subgraphs nest by
  `EpicEntry::parent` and only those holding a node are drawn; epic nodes use the `[[…]]` shape,
  statuses are `classDef`s (`in_progress` etc., only those used), cyclic edges are red (`linkStyle`).
- **Bounded document.** `MarkdownOptions { max_graph_nodes, max_graph_edges, max_graph_chars,
  max_document_chars }`. With the defaults `render` never returns more than
  `DEFAULT_MAX_DOCUMENT_CHARS` = 60 000 bytes, under the 65 536 characters of a PR comment, whatever the
  store's size (a job-summary caller can raise it; 1 MiB there). The header and summary counts are
  always complete; the graph section gets at most half of what is left, and the tables then share the
  rest in document order. A table that does not fit ends after its last whole row with
  `_…and N more not shown._`, board columns and graph parts that no longer fit become
  `_…and N more columns/parts not shown._`, a section with no room is left out, and a `**Shortened.**`
  note under the summary says so. Nothing is ever cut inside a row, a `<details>` block or a fence.
  Titles are cut to `MAX_CELL_CHARS` (200) in tables and headings, `MAX_LABEL_CHARS` (40) in nodes.
- **Bounded graph.** Defaults `DEFAULT_MAX_GRAPH_NODES` = 100, `DEFAULT_MAX_GRAPH_EDGES` = 200 (Mermaid's
  own cap is 500) and `DEFAULT_MAX_GRAPH_CHARS` = 30 000 bytes of Mermaid text for the whole document
  (Mermaid refuses 50 000 per diagram), never more than the graph section's share. A graph over the
  node/edge bounds, or whose one diagram would be over the text budget, is drawn per top-level epic
  plus "Not in an epic", with a note naming the bound it exceeded and counting the dependencies
  between parts that are not drawn; a part still over the node/edge bounds ("too large to draw here")
  or the remaining text budget ("no room left") becomes a note with its size.
- **Untrusted text.** Titles, tags, assignees and IDs go to one line (control characters and line
  breaks become spaces). In Markdown, backslash, backtick and `* _ [ ] ~ | $ # !` are backslash-escaped, `< > &` become
  entities and `@` is followed by a word joiner so it cannot mention anyone; well-formed IDs are code
  spans, malformed ones are escaped as text. In Mermaid labels every ASCII character other than
  letters, digits, space and `.,-_:/'!?+=*@$^~` becomes an entity code (`#35;`), so no quote, bracket,
  `;`, `%%`, backtick or fence can end a label or add a statement.

### Snapshot HTML renderer

`snapshot::html::render(&Snapshot) -> String` turns a snapshot into one self-contained HTML page. It is
pure and deterministic (no store, no clock; the time shown is `generated_at`) and builds without the
`cli` feature. Its title ("Bears snapshot") and time line (`Generated YYYY-MM-DD HH:MM UTC.`) are
those of the Markdown header.

- **Template.** `templates/snapshot/snapshot.html` (embedded as `snapshot::html::TEMPLATE`) holds the
  document and all CSS; `render` fills its `{{generated}}` and `{{sections}}` placeholders in one pass,
  so text inside a value is never treated as a placeholder. The file is in the `cargo package`
  guard in `tests/cli.rs`.
- **Sections.** Header stats (totals, queue sizes, a stacked bar per status and per outstanding
  priority), epic progress bars (tree order, indented by depth, coloured by children's status), the
  ready / in-progress / review queues, the status board, the dependency graph, and — only with
  `include_all` — the recent archive log ("The archive is empty." when there is none).
- **Graph.** An inline `<svg>` drawn from `SnapshotGraph`: one column per layer, nodes at
  `(layer, position)`, edges as curves from the dependency's right side to the dependent's left side
  with an arrowhead; cyclic edges (same layer) loop out to the right and are dashed, cyclic nodes and
  epics are outlined differently. Titles in nodes are cut to `NODE_TITLE_CHARS` characters; the full
  text is in each node's `<title>` tooltip.
- **Self-contained and safe.** Inline CSS only, no JavaScript, no fonts, images, scripts or URLs
  (the only `url(` references are `url(#arrow)` and `url(#arrow-cyclic)`, the SVG arrowhead
  markers; inline SVG needs no `xmlns`), light/dark via `prefers-color-scheme`. Every piece of task
  content is escaped with `snapshot::html::escape` (`& < > " '`), in text, attributes and inside the
  SVG.

### `bea snapshot` CLI

`bea snapshot [--format html|md] [-o/--output FILE] [-a/--all]` (`cmd_snapshot` in `cli/cmd.rs`,
`SnapshotFormat` in `cli/args.rs`) is a thin frontend over `bears::snapshot`: it calls
`snapshot::load` with `include_all = --all`, the default `archive_limit` and `generated_at =
Utc::now()` (the only non-deterministic part of the output), then `markdown::render` (`md`, the
default) or `html::render`. Output goes to stdout with `print!`, byte for byte what `-o` writes, so
`bea snapshot > f` and `bea snapshot -o f` give the same file; with `-o` nothing is printed. The
file is overwritten if it exists, and a missing directory is an `Io` error naming the path (never
created). The store is loaded before anything is written, so `NotInitialized` / `LegacyStore` fail
with the usual hints and leave no file.

- **Never into the store.** An `-o` path whose *resolved* location is `.bears/` or below
  (`refuse_output_in_store`: the parent directory is canonicalised and the file name appended, so
  `./.bears/x`, absolute paths, `..` and symlinked directories are seen through, while
  `out/.bears-report.md` or another directory named `.bears` is fine) is refused with `InvalidUsage`
  naming the path, before anything is written: every loading command would otherwise warn about the
  file and `bea doctor` fail. A parent that does not exist is left to the write's `Io` error.
- **Atomic write** (`write_atomically`, std only; the store's `write_task_file` uses a fixed temporary
  name and does not fit): a fresh `.{name}.{pid}-{nanos}-{n}.tmp` beside the target, created with
  `create_new` (so concurrent runs to one target never share it), synced, then renamed over the
  target; it is removed if writing or renaming fails, and nothing else is left in the directory. An
  existing regular file's permissions are carried over (not its owner); an existing symlink at the
  target is replaced by the file, never written through. There is deliberately no `--json` (the global flag is
refused with `InvalidUsage`), no size or archive-limit flag, no MCP tool and no TUI view. It is
dispatched before the shared `load_all`, since `snapshot::load` reads active tasks and archive itself.

### Cycle detection

Validate on `dep add` that the new edge doesn't create a cycle. Reject with a clear error if it would.
Cycles that got in anyway (hand edits, merges) are found by `Graph::cycles()` (iterative Tarjan,
O(V+E)), which `bea doctor` reports.

### `bea doctor` — store check and repair

The loaders paper over some inconsistencies (a duplicate ID collapses to one task, an unparsable
file is skipped with a warning), and a git merge of two branches can produce others. The doctor
reads every task file itself, active and archive, through `store::scan_task_files` (keeps every
file, parse errors included, prints nothing), so it sees them.

- `service::check_store(base) -> StoreReport { files, findings, renamed, moved }` is read-only.
  `Finding` is an enum serialized with a kebab-case `kind` tag, reported in this order: `legacy-store`
  (any `parent:` key: reported **alone**, nothing else is checked, `bea migrate` is the fix),
  `unreadable-file`, `malformed-id` (`validate_id`), `duplicate-id` (across active and archive; the
  copy that keeps the ID listed first, with each copy's title, and the `dependents` / `descendants`
  that refer to the ID), `filename-mismatch` (the filename's ID differs from the
  task's; a stale slug is not a finding), `dangling-dependency` (names no task, active or archived),
  `missing-parent` (delete and prune detach an epic's children, so this comes from hand edits or
  merges), `parent-not-epic`, `dependency-cycle`. Paths are relative to the base.
- `service::repair_store(base)` fixes the two `Finding::is_fixable()` kinds, then checks again and
  returns what remains. **Duplicates:** the copy that keeps the ID is the active one before an
  archived one, then the one whose filename starts with the ID, then the first by path (what
  `find_task_path` already finds); every other copy gets a fresh ID at the same place in the
  hierarchy (child ID under the same parent, or a root ID), in its own directory, keeping title,
  body and `updated` (`store::rewrite_task_file`, path-addressed, since `rename_ids` cannot tell two
  files with one ID apart). **References are deliberately not rewritten:** a `depends_on` naming
  the ID, or a task below it, records only the ID, so nothing says which copy it meant, and a guess
  would silently move it. They all stay with the kept copy, and each
  `Renumbered { from, to, file, title, kept_file, kept_title, dependents, descendants }` lists
  them so the user can check them and move any that belong to the renumbered copy. Duplicates of a
  malformed ID are left alone. **Filenames:** a misnamed file is renamed to `{id}-{slug}.md`
  (`MovedFile { id, from, to }`) unless a file is already there. A legacy store is not touched.
  Everything else is report-only. `rewrite_task_file` rewrites the content in place first and
  renames second, so an interrupted repair leaves at worst one misnamed file (which the next run
  just moves), never a second copy that would be renumbered into another task.
- `bea doctor [--fix]` prints the findings (with `--fix`, first `  renamed <old> → <new> (<file>,
  "<title>")` lines, each followed by the kept copy and the tasks left attached to it, and
  `  moved <from> → <to>` lines) and **exits 1 when problems are found** — after `--fix`, only
  when problems remain — so it can gate CI. A fix that left tasks attached to a kept copy still
  exits 0 (the store is consistent) but ends by telling the user to review them. `--json` prints the report plus `ok` and
  `needs_review` (both always present; `needs_review` is `StoreReport::needs_review()`, `true` exactly
  when a fix in this run renumbered a copy whose `dependents` or `descendants` stayed with the kept
  copy, so always `false` without `--fix`; it changes neither `ok` nor the exit status); with `--fix`,
  `renamed` (a list, since one old ID can have several copies) and `moved` are always present. It
  runs before `load_all`, so it works on a legacy store. The MCP `doctor` tool returns
  `{ ok, files, findings }` and has no fix parameter.

### Archive

Archived tasks live in `.bears/archive/` — a subdirectory created by `bea init`. `store::load_all` scans only the top-level `.bears/` directory and therefore **never** returns archived tasks; the archive is invisible to all normal operations.

**Archivability invariant.** A task is archivable if and only if:
1. Its status is `done` or `cancelled` (terminal), **and**
2. No active (not done/cancelled) task in the active store depends on it.

Violating either condition causes `archive_task` to return `Error::NotArchivable` naming the blockers.

**Cascade direction.**
- *Archiving an epic* automatically also archives its settled (done/cancelled) direct children (not the tasks below a nested epic).
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

**`prune` is hard-delete; archive is the soft option.** `prune_tasks` / `bea prune` permanently deletes active cancelled (and optionally done) tasks. It never deletes from the archive (it only renames archived children of a pruned epic, see Epic behavior). Use archive/restore when history should be recoverable.

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
| `list_stale` | `hours?` (default 24) |
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
| `doctor` | — (read-only store check) |

On `update_task`, a `parent` other than the current one (`""` for none) moves the task and its subtree to new IDs; the result then carries `renamed: { old: new }`. `delete_task` adds the same `renamed` key when a deleted epic's children were detached; `prune_tasks` always returns `{ pruned, renamed }` (`renamed` empty when nothing moved). `get_task`'s `parent` is derived from the ID. `active_only?` (on `list_all_tasks` / `search_tasks`) excludes done/cancelled tasks. `get_task` resolves active tasks first and **falls back to the archive** when the id isn't active, marking the returned detail with `"archived": true` (read-only — restore before mutating).

## Dependencies

- `clap` (derive) — CLI parsing
- `clap_complete` — shell completion generation
- `serde`, `serde-saphyr`, `serde_json` — serialization (`serde-saphyr` is pure safe Rust; it replaced `serde_yml`, which is unsound and unmaintained per RUSTSEC-2025-0068)
- `jiff` (`std` only, no time-zone database) — timestamps (it replaced `chrono`, unmaintained per RUSTSEC-2026-0543). `chrono` is still in `Cargo.lock`, through `rmcp` (an unconditional dependency, and its `schemars/chrono04` feature), so the advisory stays ignored by id in `.cargo/audit.toml` until `rmcp` drops it; the library built without default features has no `chrono`
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
- Task ID not found → clear error (an `epic` filter naming no task or a non-epic is an error too, not an empty list)
- Cycle detected → reject with explanation
- Invalid frontmatter → warn and skip (don't crash), report which file, and what is wrong in it and where, in the file's terms (see Storage format), not the YAML library's
- Library code uses `Result` with `?`. CLI formats errors for humans. MCP returns `rmcp::ErrorData` from tool methods.
- **Broken pipes.** Rust ignores `SIGPIPE` at startup, so `println!` to a closed pipe (`bea show | head`)
  panics instead of exiting. `restore_sigpipe()` in `main.rs` puts the default disposition back before
  the CLI runs — the process then dies from the signal like any other Unix filter. It is **not** applied
  to `bea mcp` or `bea tui`, which own their transports. This is why the ~70 `println!` calls in
  `cli/cmd.rs` can stay as they are; there is a regression test in `tests/cli.rs`
  (`test_broken_pipe_exits_quietly`, unix-only). Windows has no `SIGPIPE` and is not covered.

## Testing

- Unit tests in `graph.rs`: cycle detection, topological sort, ready computation
- Unit tests in `task.rs`/`store.rs`: frontmatter parsing (valid, missing fields, extra fields, malformed); the exact on-disk frontmatter shape, parse errors pinned whole (null into a string field, duplicate key, a value where a list belongs and the reverse, a list or mapping for status/priority/type, unknown status/priority/type, a number too large, a bad number, a bad or null timestamp, a missing field, broken YAML; no library advice), so a library upgrade that rewords one fails a test, and a render/parse round-trip over every file in this repository's own `.bears/` and `.bears/archive/` (unchanged except that a legacy `parent:` key is dropped); archive storage layer (`move_to_archive`, `move_from_archive`, `load_archived`, `archived_id_set`); `rename_ids` (active + archive, references rewritten, `updated` kept, untouched files left alone, swap/chain/identity maps refused, validation before any write); legacy stores (both loaders refuse a `parent:` key in either directory; the line pre-filter ignores `parent:` text in a body, block scalar or indented value, unusual valid spellings are still caught, and an unparsable file with a `parent:` line is never legacy for either loader) and `migrate` (nested epics, archived child of an active epic and the reverse, references and `updated`, missing/non-epic/cyclic parents kept as roots, dry run writes nothing and shows the real run's map, two copies of a store migrated to identical files, second run a no-op, resume from every interruption point incl. an already-removed epic); and this repository's own `.bears/`, already migrated, checked as a copy: it loads, has no `parent:` key, `migrate` is a no-op on it, every derived parent is an existing epic and every `depends_on` names a task
- Unit tests in `snapshot/tests.rs`: counts (every status and priority, epics not counted, independent of
  `include_all`), queues equal to the `service` listings, ties broken by ID whatever the map order,
  determinism and caller-supplied `generated_at`, board columns with and without `include_all`, epics in
  tree order with progress and the nearest shown epic, longest-path layering, ordering by dependency
  position, settled/missing dependencies left out, cycles in one layer, epics as nodes only in an edge,
  effective priority, the archive log, the JSON shape, `load` (incl. not initialized), and a snapshot of
  this repository's own store, taken on a copy and skipped when `.bears/` is absent (packaged crate)
- Unit tests in `snapshot/markdown/tests.rs`: every section (empty store, `generated_at`, counts,
  epics with progress and nesting, queues, effective priority, board columns and the archive only with
  `include_all`), determinism over map order, nested subgraphs with generated IDs, an epic node distinct
  from its subgraph, status classes, cycles, hostile titles/tags/assignees/IDs that cannot break a table
  row, start a mention or code block, or end a Mermaid label or add a statement, label truncation, and
  the graph bounds (one diagram within them, per-epic parts over them with node IDs kept, a part over
  the bounds and an exhausted text budget as notes, a graph over only the text budget saying so, the
  defaults within GitHub's limits), and the document budget (a 1 500-task store with hostile titles
  within 60 000 bytes and well-formed with the counts complete, a raised budget showing everything, a
  zero budget leaving the summary and the note, a sweep of budgets never exceeded nor breaking a row,
  `<details>` block or fence, a cut table ending on a whole row with its count, titles cut in tables)
- Unit tests in `snapshot/html/tests.rs`: escaping, one-pass template filling, deterministic output,
  generated time and title as in the Markdown header, light/dark CSS, no external references (no URL, `src`/`href`, script, link, event
  handler, only in-document `url(#…)`), hostile titles/tags/assignees/IDs escaped in text, attributes
  and the SVG, sections (archive only with `include_all`, empty archive), header counts, epic progress
  bar widths, queues, board columns and cards, graph nodes placed at their layer and position with
  every edge drawn (cyclic ones marked), epic nodes, title truncation by character
- Store doctor: `graph.rs` (`cycles()`: one entry per component, self-loops, DAG, a 50 000-task chain);
  `service.rs` (`check_store`: healthy store, every finding kind in order, legacy store reported alone
  and untouched by a repair, not initialized; `StoreReport::needs_review`; `repair_store`: root and child duplicates renumbered in
  place with references and `updated` kept, active copy kept over archived, filename-matching copy
  kept, misnamed file renamed, report-only findings and an occupied target left as they were);
  `mcp/tools.rs` (`doctor` read-only, findings are not a tool error); `tests/cli.rs` (`bea doctor`
  exit status 0/1, `--fix` and `--json` output, `needs_review` true only after a fix that left
  references to review, problems remaining after `--fix`, legacy store with
  migrate hint, not initialized); `tests/mcp.rs` (tool listed)
- Unit tests in `task.rs`/`store.rs` for hierarchical IDs: `derive_child_id` (deterministic with a pinned value, shape, growth, collisions, YAML-safe), validation (flat and dotted, malformed rejected), segments/depth/`parent_of`/`root_of`/`leaf_segment`/`is_descendant`, `id_from_filename`, child-segment growth threshold (only direct children of that length count; lengths coexist after growth), collision avoidance, skipping YAML-unsafe IDs, dotted-ID round-trip, and dotted IDs in `find_task_path`/`resolve_prefix`/`archived_id_set`
- Unit tests in `config.rs`: `id-length` and `child-id-length` defaults, loading, parse errors worded like frontmatter errors, bounds validation, and serialisation (both keys always written, a file without `child-id-length` loading with the default and not rewritten)
- Unit tests in `task.rs`: parent derived from the ID, no `parent:` key written, legacy `parent:` key read into `legacy_parent` but never written or used as the parent; `in_epic` (the subtree, whole segments, the epic itself excluded)
- Unit tests in `service.rs`: epic progress, auto-close (incl. cancelled children and nested cascade); `epic` filters over a nested epic (list/ready/review match the whole subtree, whole segments only) while progress, auto-close and the archive cascade stay on direct children; `resolve_epic_filter` (exact, prefix past the epic's children, ambiguous, unknown, empty, non-epic, archived-only epic by ID and prefix, active tasks tried before the archive, an archived epic with several active children, the missing epic of one or several orphans not found); child creation (hierarchical ID under the resolved parent, nested epics, `child-id-length`, archived-ID avoidance, ancestors of orphans reserved at root and child level); reparenting (subtree moved with archived descendants and active/archived dependants rewritten, detach to a fresh root ID, root task moved under an epic, auto-close at the new parent, self/descendant/non-epic/unknown parents refused before any write), unchanged parent accepted; delete/prune (dangling `depends_on` scrubbed, an epic's active and archived children detached to fresh roots with subtrees and references, prune detaching only survivors incl. a pruned task inside a detached subtree); start/release (assignee set/keep/clear, release rejects non-`in_progress`); attempts (counted on every path into `in_progress`, not on restart/release/other transitions); review/proposal workflow (edge preconditions, review not unblocking dependents or closing epics, proposals hidden from listings); archive service (`is_archivable`, `archive_task`, `archive_all`, `restore_task`, `list_archive`, `get_archived_task`, archived-ID collision avoidance)
- Unit tests in `service.rs` for `list_stale`: only `in_progress` tasks at or past the threshold (exactly the
  threshold counts, zero lists every claim), an `updated` after `now` never stale, idle hours rounded down,
  canonical order with ID ties, the JSON summary (`assignee`, `attempts`, `updated`, `idle_hours`); `mcp/tools.rs`
  (`list_stale` default and `hours`, nothing released); `tests/cli.rs` (`bea stale` empty message, human
  line with assignee and idle hours, `--json`, `--hours 0`, nothing written, a negative value refused);
  `tests/mcp.rs` (tool listed)
- `bea prime`: `service.rs` (`prime` keeps the first ready tasks in `list_ready` order and counts the
  rest, a zero limit only counts, in-progress and review queues, the JSON shape with assignee and
  attempts); `tests/cli.rs` (empty store, sections with assignee and attempts — `attempt N` in
  progress, earlier attempts on ready and review rows — one ID width across sections, the "not shown" note,
  no colour when piped, `--json`, not initialized and the documented hook command a no-op there,
  completions)
- Unit tests in `mcp/tools.rs`: tool create/list/start/complete/search/graph/plan_epic/delete/deps/validation; delete/prune of an epic (`renamed` map, `{ pruned, renamed }`, the same object with empty members when nothing moved or nothing was pruned); derived parent and reparenting (`renamed` map, detach, refusal under its own subtree); archive tools (archive/restore/list_archived); end-to-end archive visibility (hidden from list/search/graph/epics), integrity (dep add onto archived ID, prune never touches archive, no ID reuse)
- Unit tests in `graph.rs`: effective-priority correctness, `ready` breaking exact ties by ID, DAG dep-tree bounding, bounded adjacency (its `epic` filter spanning the subtree), the `ready` epic filter over nested epics, plus `#[ignore]`d coupled-graph perf benchmarks (run with `cargo test -- --ignored`)
- Unit tests in `scaffold.rs`: `.mcp.json` merge (preserve/idempotent/fresh) and harness scaffolding (claude/copilot/codex skill/agent files, `bea mcp` binary form); every template describes hierarchical IDs and the `renamed` map, the planning skill and CLI fallback say the `epic` filter reaches nested epics, and none mentions a `parent:` key or a CLI spelling that does not exist
- Unit tests in `tui/`: input truncation (UTF-8 safety), ready-filter parity, task-list titles aligned across ID lengths, subtask tree expanding nested epics once, the renames modal (deleting an epic lists every detached child's old and new ID, nothing shown when nothing moved, scrolling bounded so every row is reachable, closed only by Esc/Enter/q), watcher (the watcher test is `#[ignore]`d as timing-sensitive)
- Integration tests in `tests/cli.rs`: create a temp `.bears/` dir, run commands, verify file output; an unparsable frontmatter's load warning and `bea doctor` finding carrying the reworded message; hierarchical child IDs and `update --parent` moves (renamed output and `--json` map, archived dependant rewritten, detach, no-op, self-parent refused); `delete`/`prune` of an epic (children detached, renamed lines, `--json` `renamed` map and `{ pruned, renamed }`, also when nothing moved or nothing was pruned); legacy store refused by every loading command with the `bea migrate` hint and nothing written; `bea migrate` (`--dry-run`, `--json` map equal to the dry run's, two copies migrated identically, orphan report, idempotent re-run, not initialized); archive/restore/log/list-archived e2e; `bea init --claude/--copilot/--codex` scaffolding e2e (file presence, idempotency, MCP merge, `bea mcp` binary form, template packaging guard); list output aligned across dotted-ID lengths; exact priority/creation ties in `ready`/`list`/`search` (and a `--limit` cut) ordered by ID; `list`/`ready`/`review --epic` over a nested epic, by prefix, unknown or non-epic values refused, and an archived-only epic with two active children; `bea snapshot` (Markdown by default and `--format md`, HTML, `--all` adding done and archived work which the default leaves out, `-o` writing what stdout prints and printing nothing, an existing file overwritten, a missing directory refused, a path inside `.bears/` refused however it is spelled (relative, absolute, `..`, a symlinked directory; unix-only for symlinks) while names that only mention `.bears` are written, no temporary file left after success or a failed rename, concurrent writes to one target leaving one whole snapshot, permissions kept and a symlink target replaced (unix), an unknown format and `--json` refused, not initialized and legacy store failing with their hints and no file written, a hostile title escaped in both formats, the command in shell completions); tests that compare two snapshot runs drop the "Generated" line, the only time-dependent one
- E2E tests in `tests/mcp.rs`: spawn the real `bea mcp` binary and drive it over stdio with the
  rmcp client (dev-dependency features `client`, `transport-child-process`) — handshake, tool
  list/schemas, tool calls, the `epic` parameter of `list_ready`/`list_all_tasks`/`list_review`/
  `get_graph` over a nested epic and by prefix (unknown and non-epic values a tool error), protocol- vs tool-level error semantics, and a legacy store refused as a
  tool error
- MCP tools also verified end-to-end via live MCP sessions
