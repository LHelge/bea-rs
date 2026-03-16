---
id: '622'
title: 'MCP: epic support in tools'
status: open
priority: P1
created: 2026-03-16T21:40:22.226461809Z
updated: 2026-03-16T21:42:57.386981211Z
tags:
- feature
- epic
depends_on:
- vcq
- adw
parent: hza
---

Update MCP tools for epic support:
- `create_task`: add optional `type` parameter (default "task")
- `list_ready`: add optional `epic` parameter (filter by parent epic ID)
- `list_all_tasks`: add optional `epic` parameter
- Include `type` in `TaskSummary` and `TaskDetail` MCP responses
- Consider adding a `list_epics` convenience tool