---
id: rk4
title: Add type field (task/epic) to Task model
status: open
priority: P1
created: 2026-03-16T21:40:22.196230577Z
updated: 2026-03-16T21:42:57.364872982Z
tags:
- feature
- epic
parent: hza
---

Add a `type` field to the `Task` struct with two variants: `task` (default) and `epic`.

- Add a `TaskType` enum with `serde` support (lowercase serialization)
- Default to `task` when field is missing (backward compat with existing task files)
- Add `JsonSchema` derive for MCP parameter support
- Update `TaskSummary` and `TaskDetail` projections to include type