---
id: 98q
title: Use VecDeque in topo_sort_subset and document priority tie-break
status: done
priority: P3
created: 2026-05-30T21:39:12.154941301Z
updated: 2026-05-31T04:29:21.950626838Z
tags:
- performance
- refactor
parent: nya
---

`src/graph.rs:219` uses `queue.remove(0)` (O(n) front-pop) in a loop → O(n²). Switch to `VecDeque::pop_front`. Also note the global priority tie-break isn't strictly maintained: newly-ready nodes are sorted among themselves and appended rather than merged into the remaining queue, so `--plan` ordering is only approximately priority-sorted. Either merge for a globally-correct order or add a comment that approximate ordering is intentional.