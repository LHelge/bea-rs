---
id: syf
title: Add --plan flag to Show command and cmd_show handler
status: open
priority: P1
created: 2026-03-18T22:55:30.214265842Z
updated: 2026-03-18T22:55:30.214265842Z
tags:
- cli
depends_on:
- axs
parent: 27y
---

## Summary
Add a `--plan` flag to the `Show` CLI command. When set, instead of showing the task detail, output the subtasks of the given parent in topological execution order as numbered markdown headings with the task body content. With `--json`, output an ordered JSON array.

## Acceptance Criteria
- [ ] `bea show <id> --plan` outputs children as numbered markdown: `# 1. Title\n\n<body>\n\n`
- [ ] `bea show <id> --plan --json` outputs a JSON array of task details in topological order
- [ ] Without `--plan`, `bea show <id>` behaves exactly as before (no regression)
- [ ] Tasks with empty bodies get just the heading with a blank line after
- [ ] Numbering starts at 1 and increments sequentially
- [ ] If the parent has no children, output nothing (markdown) or `[]` (JSON)

## Implementation Notes

### `src/cli/args.rs`
- Add a `--plan` flag to the `Show` variant:
  ```rust
  Show {
      /// Task ID
      id: String,
      /// Output subtasks in execution order as markdown
      #[arg(long)]
      plan: bool,
  },
  ```

### `src/cli/mod.rs`
- Update the pattern match to destructure `plan` and pass it to `cmd_show`:
  ```rust
  Command::Show { id, plan } => cmd::cmd_show(&tasks, &id, plan, cli.json),
  ```

### `src/cli/cmd.rs`
- Update `cmd_show` signature to accept `plan: bool`
- When `plan` is true:
  - Call `service::plan_epic(&tasks, id)?` to get ordered children
  - If `json`: serialize as a vec of `task.detail()` objects
  - If not `json`: iterate with `enumerate()`, print `# {n}. {title}\n\n{body}\n\n` for each
- When `plan` is false: existing behavior unchanged

### Markdown output format
```markdown
# 1. First task title

Task body content here.

# 2. Second task title

Another task body.
```

### JSON output format
```json
[
  { "id": "abc", "title": "First task title", "status": "open", ... },
  { "id": "def", "title": "Second task title", "status": "open", ... }
]
```

## Edge Cases & Considerations
- Empty body: still print the heading and a blank line (no body paragraph)
- `--plan` on a task with no children: empty output / `[]`
- `--plan` without specifying an ID: clap handles this with a required positional arg (no change needed)

## Testing
- Add integration tests in `tests/cli.rs`:
  - Create an epic with 3 linearly-dependent children, run `bea show <epic> --plan`, verify markdown output order
  - Same with `--json`, verify JSON array order
  - `bea show <epic> --plan` with no children outputs nothing
  - `bea show <task> --plan` on a regular task with children also works
- Verify `bea show <id>` without `--plan` still works (regression test)
- `cargo fmt && cargo clippy && cargo test` must pass

## References
- `cmd_show` current implementation: `src/cli/cmd.rs` line 185
- `Show` variant in args: `src/cli/args.rs` line ~99
- Dispatch in `src/cli/mod.rs` line ~53
- Task `axs` — `service::plan_epic` function
