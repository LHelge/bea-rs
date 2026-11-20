---
id: hza.e8
title: Add epic/parent filter to list_ready and list_all
status: done
priority: P1
created: "2026-03-16T21:40:22.217853765Z"
updated: "2026-03-16T22:37:31.659354559Z"
tags:
  - feature
  - epic
depends_on:
  - hza.x2
  - hza.fz
---

Add `epic` / `parent` filter parameter to:
- `list_ready(epic?)` — only return tasks that are children of the given epic
- `list_all_tasks(epic?)` — filter by parent epic

This lets an agent say "give me the next ready task for epic X".

Implement in service layer, expose through both graph ready computation and list filtering.