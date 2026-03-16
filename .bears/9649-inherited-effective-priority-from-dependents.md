---
id: '9649'
title: Inherited effective priority from dependents
status: open
priority: P2
created: 2026-03-16T10:18:51.775377Z
updated: 2026-03-16T10:18:51.775377Z
tags:
- feature
---

Implement inherited/effective priority. A task's effective priority should be the highest (lowest number) of its own priority and the priorities of all tasks that depend on it (directly or transitively).

For example, a P3 task that blocks a P1 task should display as effectively P1, since completing it is on the critical path for the P1 task.

- Add an `effective_priority(task_id, graph, tasks)` function that walks the reverse dependency graph to find the highest inherited priority
- Update `bea ready` to sort by effective priority instead of intrinsic priority
- Update `bea list` and `bea show` to display effective priority when it differs (e.g. `P3 → P1`)
- Update MCP tool responses to include `effective_priority` alongside `priority`
- Add unit tests for chains (P3 blocks P2 blocks P0 → all effective P0) and diamond dependencies