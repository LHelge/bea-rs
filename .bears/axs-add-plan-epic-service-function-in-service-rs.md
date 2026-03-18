---
id: axs
title: Add plan_epic service function in service.rs
status: open
priority: P1
created: 2026-03-18T22:55:27.025664694Z
updated: 2026-03-18T22:55:27.025664694Z
tags:
- service
depends_on:
- w58
parent: 27y
---

## Summary
Add a `plan_epic` function to `service.rs` that, given a parent task ID, collects all child tasks, builds the dependency graph, and returns them in topological order. This bridges the graph algorithm (from `w58`) to the CLI layer.

## Acceptance Criteria
- [ ] `plan_epic(tasks: &HashMap<String, Task>, parent_id: &str) -> Result<Vec<&Task>>` returns children in topological execution order
- [ ] Returns an error if `parent_id` doesn't exist in tasks (uses existing `Error::TaskNotFound`)
- [ ] Works for any task with children — not restricted to `TaskType::Epic`
- [ ] Returns an empty vec if the parent has no children
- [ ] Includes children of all statuses (open, in_progress, done, cancelled, blocked)

## Implementation Notes
- Add to `src/service.rs`
- Steps:
  1. Validate `parent_id` exists in `tasks` (reuse `get_task` or inline the check)
  2. Collect child IDs: `tasks.values().filter(|t| t.parent.as_deref() == Some(parent_id)).map(|t| t.id.clone()).collect::<HashSet<_>>()`
  3. Build graph: `Graph::build(tasks)`
  4. Call `graph.topo_sort_subset(&child_ids, tasks)` (from task `w58`)
  5. Return the sorted vec
- Follow existing patterns: `list_ready()` and `epic_progress()` in `service.rs` show how children and graph interact

## Edge Cases & Considerations
- A parent with no children should return `Ok(vec![])`, not an error
- The parent task itself is NOT included in the output — only its children
- Children that depend on tasks outside the epic (external deps) are still included; only intra-epic dep ordering matters

## Testing
- Add tests in the `#[cfg(test)] mod tests` block in `src/service.rs`
- Use existing `make_task` / `make_epic` helpers
- Test cases:
  - Epic with 3 linearly-dependent children → correct order
  - Epic with independent children → all returned (any order)
  - Epic with no children → empty vec
  - Non-existent parent ID → error
  - Non-epic parent with children → still works
- `cargo fmt && cargo clippy && cargo test` must pass

## References
- `service::epic_progress()` — pattern for collecting children by parent: `src/service.rs`
- `service::list_ready()` — pattern for using Graph: `src/service.rs`
- Task `w58` — `Graph::topo_sort_subset` implementation
