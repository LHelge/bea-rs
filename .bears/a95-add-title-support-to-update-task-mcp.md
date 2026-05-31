---
id: a95
title: Add title support to update_task (MCP)
status: done
priority: P2
created: 2026-05-30T22:01:10.302417518Z
updated: 2026-05-31T06:59:29.574863037Z
tags:
- mcp
- api
parent: 2kx
---

`update_task` cannot change a task's title. `src/mcp/tools.rs:146` passes `None` with the comment "MCP doesn't support title update currently", and `UpdateTaskParams` (src/mcp/params.rs:51-65) has no title field. `service::update_task` already accepts `title: Option<String>`, so the change is small.

Fix:
- Add `title: Option<String>` (with a doc comment) to `UpdateTaskParams`.
- Pass `params.title` through in the `update_task` tool instead of `None`.

Prefer extending `update_task` over adding a separate `update_title` tool — a dedicated tool would just duplicate update_task. Add a tool test (rename a task, assert the summary reflects the new title and the file is renamed).