---
id: a42
title: Add a large coupled-graph benchmark to repro and locate graph timeouts
status: done
priority: P2
created: 2026-05-30T21:47:29.585840119Z
updated: 2026-05-31T04:29:23.361666367Z
tags:
- performance
- graph
- test
parent: 4vq
---

Build a repro/benchmark harness that generates N densely-coupled tasks and measures the graph-related commands and MCP tools, then keep it as a regression guard.

Baselines already measured (release, synthetic DAG, MAXDEP~60):
- 2000 tasks / 59k edges: graph --json 85ms (863 KB out); ready/list/show ~3.0s each.
- 8000 tasks / 239k edges: graph --json 0.30s (3.5 MB out); ready ~73s.

Conclusions the harness should lock in:
- get_graph compute is linear; its problem is payload size (pjg).
- ready/list/show/get_task are quadratic via effective_priorities (f6r).
- the human `bea graph`/`dep tree` blow up via exponential dep_tree (dbt) — add a coupled-but-shallow case that would explode pre-fix.

Deliverable: a bounded regression test (or `--release`/ignored bench) that fails if these scale super-linearly. Targets post-fix: ready/list/show < 1s and get_graph payload bounded at 8000 tasks. A generator script is in the task body history / can be reconstructed (Python writing .bears markdown files).