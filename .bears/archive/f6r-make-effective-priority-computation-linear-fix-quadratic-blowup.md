---
id: f6r
title: Make effective-priority computation linear (fix quadratic blowup)
status: done
priority: P1
created: 2026-05-30T21:39:07.451176658Z
updated: 2026-05-31T04:29:22.409296482Z
tags:
- performance
- refactor
parent: 4vq
---

Measured on a synthetic densely-coupled DAG (release build):
- 2000 tasks / 59k edges: `ready`/`list`/`show` ~3.0s each.
- 8000 tasks / 239k edges: `ready` ~73s. 4x the tasks => ~24x the time, i.e. quadratic.

Root cause: `service::effective_priorities` (src/service.rs:265-272) calls `graph.effective_priority` once per task, and each call BFSes the reverse graph over all (transitive) dependents => O(V*(V+E)). `Graph::ready` also recomputes it inside the sort comparator (src/graph.rs:62-66), and `cmd_ready`/`cmd_list`/`cmd_show` rebuild the graph a second time via `effective_priorities`. This is what makes the most-used MCP tools (list_ready, list_all_tasks, get_task) hang on large projects.

Fix: compute ALL effective priorities in a single linear pass.
- effective(x) = min(own(x), min over direct dependents y of effective(y)).
- Process nodes in reverse-topological order (dependents before dependencies) and propagate in one sweep => O(V+E).
- Handle cycles: the current per-node BFS tolerates them via a visited set; a topo pass needs an explicit cycle/SCC fallback.
- Build the graph once per command and thread it plus the precomputed map through, instead of rebuilding in cmd_* and recomputing inside the ready comparator.

Target: linear scaling; ready/list/show well under 1s at 8000 tasks. Guard with the large-graph benchmark (a42).