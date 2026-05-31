---
id: 5wj
title: 'MCP tools: archive_task / restore_task / list_archived'
status: done
priority: P2
created: 2026-05-30T22:13:43.945605056Z
updated: 2026-05-31T07:39:24.491161315Z
tags:
- feature
- archive
- mcp
depends_on:
- z54
parent: h4j
---

Depends on the service layer (z54). src/mcp/params.rs + tools.rs:

- `archive_task { id? }`: with id => archive + cascade; without id => sweep archivable. (Or a separate `archive_done` sweep tool — pick one and document which.)
- `restore_task { id }`.
- `list_archived {}` -> archived task summaries.
- All other tools remain active-only by default (free, since archived tasks aren't loaded).
- Update the server instructions / tool descriptions to mention archiving.

Tool tests: archiving hides a task from list_ready/list_all_tasks; list_archived shows it; restore brings it back and it reappears in ready.