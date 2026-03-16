---
id: f2c5
title: Add epic support
status: open
priority: P2
created: 2026-03-16T10:16:45.983560Z
updated: 2026-03-16T10:16:45.983560Z
tags:
- feature
---

Add first-class epic support. An epic is a task with `type: epic` that groups related tasks via the existing `parent` field. Epics provide:

- A `type` field on tasks: `task` (default) or `epic`
- `bea epic create <title>` shorthand for creating an epic
- `bea epic list` to list epics with progress summaries (e.g. 3/7 done)
- `bea epic show <id>` to show an epic with all its child tasks and completion %
- Progress is computed from child task statuses (done/cancelled = complete, others = incomplete)
- Epics should appear distinctly in `bea list` and `bea graph` output
- MCP tools: `list_epics`, `get_epic` with progress info