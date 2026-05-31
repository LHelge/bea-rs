---
id: dbt
title: Render dependency trees as a DAG (expand each node once) to fix exponential blowup
status: done
priority: P1
created: 2026-05-30T21:47:14.296063251Z
updated: 2026-05-31T04:29:22.858799770Z
tags:
- bug
- graph
- performance
parent: 4vq
---

Root cause of `bea graph` / `bea dep tree` timeouts on coupled projects.

`graph::dep_tree_inner` (`src/graph.rs:140-170`) keeps `visiting` as a path-local set and calls `visiting.remove(id)` after recursing, so any node reachable by multiple paths is re-expanded on each path. For coupled graphs (diamonds) the produced tree is exponential in depth. `test_dep_tree_no_cycle` currently asserts the duplicated-expansion behavior, so it will need updating.

Fix:
- Add a render-global `seen` set in addition to the path-local `visiting` (cycle) set. First visit expands fully; subsequent visits emit a reference/leaf node (e.g. new `DepNode` flag `seen: true`, distinct from `cycle: true`).
- Surface the new state in both renderers: `DepNodeJson` (`src/graph.rs:275-301`), the CLI `print_tree` (`src/cli/cmd.rs:397-473`), and the TUI `render_dep_node` (`src/tui/widgets/dep_tree.rs`) — show something like `[abc] Title … (see above)`.
- Bounds output to O(V+E).

Tests: add a deep diamond / fan-in graph and assert the node count is linear, not exponential; keep cycle handling working (update `test_dep_tree_no_cycle`).

This is the shared fix that also unblocks the compact TUI rendering task.