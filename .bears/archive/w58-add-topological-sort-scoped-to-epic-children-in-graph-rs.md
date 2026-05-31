---
id: w58
title: Add topological sort scoped to epic children in graph.rs
status: done
priority: P1
created: 2026-03-18T22:55:23.701712172Z
updated: 2026-03-18T23:04:39.444140447Z
tags:
- graph
parent: 27y
---

## Summary
Add a `topo_sort_subset` method to `Graph` that performs a topological sort over a given subset of task IDs, respecting only the dependency edges that exist between tasks in that subset. This is the core algorithm needed for outputting epic children in execution order.

## Acceptance Criteria
- [ ] `Graph::topo_sort_subset(&self, ids: &HashSet<String>, tasks: &HashMap<String, Task>) -> Vec<&Task>` returns tasks in valid topological order
- [ ] Only edges between tasks in the provided subset are considered (external deps are ignored)
- [ ] Tasks with no mutual dependencies appear in a stable order (by priority, then creation date — matching existing sort conventions)
- [ ] Handles the empty-subset and single-task cases
- [ ] Handles subsets where some tasks have deps on tasks outside the subset (those deps are simply ignored)
- [ ] Unit tests cover: linear chain, diamond deps, independent tasks, single task, empty set

## Implementation Notes
- Add method to `impl Graph` in `src/graph.rs`
- Use Kahn's algorithm (BFS-based topological sort): compute in-degrees for the subset, seed queue with zero-in-degree nodes, emit and decrement neighbors
- For tie-breaking among zero-in-degree nodes, sort by priority (P0 first) then `created` (oldest first) — this matches the pattern in `Graph::ready()`
- The `self.edges` map already has all dependency info; filter to only edges where both endpoints are in the subset
- Signature suggestion: `pub fn topo_sort_subset<'a>(&self, subset: &HashSet<String>, tasks: &'a HashMap<String, Task>) -> Vec<&'a Task>`

## Edge Cases & Considerations
- If a cycle exists among the subset tasks, Kahn's algorithm will simply omit cycled nodes. Since `dep add` already rejects cycles, this shouldn't happen in practice — but document the behavior.
- Tasks in `ids` that don't appear in `self.edges` (e.g., orphan tasks with no deps) should still be included.

## Testing
- Add tests in the existing `#[cfg(test)] mod tests` block in `src/graph.rs`
- Use the existing `make_task` and `make_tasks` helpers
- Test cases:
  - `test_topo_sort_linear_chain`: A→B→C, expect [C, B, A]
  - `test_topo_sort_diamond`: A→B, A→C, B→D, C→D — expect D first, A last
  - `test_topo_sort_independent`: no deps among subset, sorted by priority/created
  - `test_topo_sort_single_task`: one task returns one task
  - `test_topo_sort_empty`: empty subset returns empty vec
  - `test_topo_sort_ignores_external_deps`: deps on tasks outside subset are ignored
- `cargo fmt && cargo clippy && cargo test` must pass

## References
- Existing `Graph::ready()` for sort-order pattern: `src/graph.rs`
- Existing `Graph::would_cycle()` for BFS traversal pattern: `src/graph.rs`
- Kahn's algorithm: BFS-based topological sort using in-degree counting
