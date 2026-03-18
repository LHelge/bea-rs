---
id: 27y
title: CLI `show --plan` for epic execution order
type: epic
status: open
priority: P1
created: 2026-03-18T22:55:20.493867579Z
updated: 2026-03-18T22:55:20.493867579Z
tags:
- cli
- graph
---

## Summary
Add a `--plan` flag to `bea show <id>` that outputs the subtasks of a parent task in dependency-respecting execution order, formatted as numbered markdown headings with task bodies. This enables piping directly to a `.md` file for a ready-made implementation plan.

## Scope
- `bea show <id> --plan` outputs numbered markdown: `# 1. Title\n\n<body>` for each child task in topological order.
- `bea show <id> --plan --json` outputs a JSON array of ordered tasks.
- Works on any task with children (not restricted to epics).
- Includes all subtasks regardless of status (open, done, cancelled, etc.).
- Tasks without mutual dependencies may appear in any order relative to each other.

## Affected Areas
- `src/graph.rs` — new topological sort method scoped to a subset of tasks
- `src/service.rs` — new `plan_epic()` function orchestrating graph + children lookup
- `src/cli/args.rs` — `--plan` flag on `Show` variant
- `src/cli/cmd.rs` — updated `cmd_show` to handle the plan output
- `src/cli/mod.rs` — pass `plan` flag through dispatch

## Acceptance Criteria
- [ ] `bea show <epic-id> --plan` outputs children in topological order as numbered markdown
- [ ] `bea show <epic-id> --plan --json` outputs ordered JSON array
- [ ] Works for any parent task, not just epics
- [ ] All child statuses included
- [ ] Parallel-safe tasks (no mutual deps) can appear in any order
- [ ] `cargo fmt && cargo clippy && cargo test` pass
