---
id: db33
title: Add bea edit command
status: done
priority: P2
created: 2026-03-15T20:47:44.260527967Z
updated: 2026-03-15T23:29:23.585808273Z
tags:
- cli
---

Open a task's .md file in $EDITOR (fall back to $VISUAL, then vi). After the editor exits, re-parse the file to validate frontmatter and report any errors. Similar to 'git commit' flow.

## Implementation Plan

### 1. Add `Edit` variant to `Command` enum (cli.rs)
- Single argument: `id: String`
- No extra flags needed (user edits the raw markdown directly)

### 2. Add `cmd_edit()` handler (cli.rs)
Flow:
1. `store::load_one(base, &id)` — load original task (validates ID exists)
2. `store::find_task_path(base, &id)` — get file path
3. Resolve editor: `$EDITOR` → `$VISUAL` → `vi`
4. Spawn editor as child process with the file path, wait for exit
5. If editor exits non-zero, bail with error
6. Re-read the file from disk and parse with `task::parse_task()`
7. If parse fails, report validation error (file is still on disk, user can fix)
8. If parse succeeds and content changed: update `task.updated` timestamp, `store::save()` (handles slug rename if title changed)
9. If unchanged, print "no changes"
10. Output task summary (respecting `--json` flag)

### 3. Wire up dispatch in `run()` match
- `Command::Edit { id } => cmd_edit(base, &id, cli.json)`

### 4. Add integration tests (tests/cli.rs)
- Test with `$EDITOR` set to a script that appends to the body → verify body changed
- Test with `$EDITOR` set to `true` (no-op) → verify "no changes"
- Test with `$EDITOR` set to a script that corrupts frontmatter → verify error reported
- Test with invalid task ID → verify error

### 5. Validate
- `cargo fmt && cargo clippy && cargo test`

### Design Decisions
- **Edit actual file, not a temp copy**: Simpler, and user can manually fix a corrupted file. `store::save()` only runs after successful parse, handling slug renames.
- **CLI-only**: This is inherently interactive — no MCP tool needed.
- **No `--json` output on success cases**: Just print task summary like `update` does.
- **Editor failure**: Non-zero exit code → print error, don't touch the file.
- **Parse failure after edit**: Print error with details, leave file as-is on disk. User can re-run `bea edit` to fix, or manually fix the file.