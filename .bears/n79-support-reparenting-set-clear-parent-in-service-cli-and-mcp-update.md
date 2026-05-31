---
id: n79
title: Support reparenting (set/clear parent) in service, CLI, and MCP update
status: done
priority: P2
created: 2026-05-30T22:01:17.462149454Z
updated: 2026-05-31T06:59:29.595200429Z
tags:
- mcp
- api
- cli
parent: 2kx
---

Today a task's parent can only be changed by editing the .bears file directly — `service::update_task` (src/service.rs:98-134) has no `parent` param, and neither CLI `update` nor MCP `update_task` exposes one.

Add reparenting end to end:
- `service::update_task`: add `parent: Option<Option<String>>` (or a small enum) so callers can distinguish "leave unchanged" / "set to X" / "clear". Simplest UX: treat an empty string as clear -> None.
- CLI: `bea update <id> --parent <id|"">`.
- MCP: add `parent` to `UpdateTaskParams` and thread it through.

Also: `create_task` never validates `parent` (only `depends_on`). Decide whether to validate that the parent exists (and optionally is an epic) on both create and reparent, and apply consistently. Add tests for set, clear, and (if added) invalid-parent rejection.