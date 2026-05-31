---
id: 4vq
title: 'Dependency graph: performance & compact rendering'
type: epic
status: done
priority: P1
created: 2026-05-30T21:46:27.667911744Z
updated: 2026-05-31T07:31:54.825437830Z
tags:
- performance
- graph
- tui
---

`bea graph` / `bea dep tree`, the TUI dep/subtask views, and several MCP tools become slow or unusable on projects with many coupled tasks.

Measured on a synthetic densely-coupled DAG (release build):
- `graph --json` / get_graph: 0.30s at 8000 tasks but a 3.5 MB payload (863 KB at 2000). Compute is linear; the payload size is the problem (see pjg).
- `ready` / `list` / `show` / get_task: ~3.0s at 2000 tasks, ~73s at 8000 — quadratic, from `effective_priorities` doing a per-node BFS (see f6r). These are the most-used MCP tools.

Three distinct issues:
1. f6r — `effective_priorities` is O(V*(V+E)); make it a single linear pass. Highest impact (affects list_ready/list_all_tasks/get_task). [P1]
2. pjg — get_graph dumps every node as adjacency JSON; bound/shape/filter the payload. This is the likely "get_graph never returns" symptom. [P1]
3. dbt — `dep_tree` expands shared nodes on every path => exponential output for the human `bea graph`/`dep tree` and the TUI. Render as a DAG (expand once). [P1]

Plus 7f2 (compact TUI rendering, builds on dbt) and a42 (large-graph benchmark/regression guard). Do a42 early so fixes are measured, not guessed.