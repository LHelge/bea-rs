---
id: hza
title: Epic support
type: epic
status: done
priority: P1
created: 2026-03-16T21:39:48.650190064Z
updated: 2026-03-16T23:16:02.280524217Z
tags:
- feature
- epic
---

Add an `epic` task type — high-level objectives with child tasks.

## Design decisions
- New `type` field on Task: `task` (default) | `epic`
- Children link to epics via existing `parent` field
- Epics auto-close when all children reach `done`
- `bea ready` excludes epics (they aren't directly workable)
- CLI: prefix epic titles with "Epic: " in bright color, show progress (e.g. [2/5])
- New `bea epics` command to list epics with progress
- MCP: `epic` filter param on `list_ready` and `list_all_tasks`; type support in `create_task`