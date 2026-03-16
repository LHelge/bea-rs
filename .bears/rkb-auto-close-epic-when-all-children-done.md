---
id: rkb
title: Auto-close epic when all children done
status: done
priority: P1
created: 2026-03-16T21:40:22.212499641Z
updated: 2026-03-16T22:27:20.468146725Z
tags:
- feature
- epic
depends_on:
- 5px
parent: hza
---

When a child task is completed (status -> done), check if all siblings under the same epic parent are also done. If so, auto-close the epic (set status to done).

- Implement in service layer (`set_status` / `complete_task` path)
- Only trigger for tasks whose parent is an epic
- Add tests for: partial completion (epic stays open), full completion (epic auto-closes), re-opening a child (epic re-opens?)