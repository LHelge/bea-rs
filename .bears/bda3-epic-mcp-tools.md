---
id: bda3
title: 'Epic: MCP tools'
status: open
priority: P2
created: 2026-03-16T10:17:23.594041Z
updated: 2026-03-16T10:21:05.201025Z
tags:
- feature
- mcp
depends_on:
- aa30
parent: f2c5
---

Add MCP tools:
- `list_epics`: list all epics with progress (completed/total counts)
- `get_epic`: full epic details including child task summaries and progress percentage

Update `create_task` to accept an optional `task_type` parameter
