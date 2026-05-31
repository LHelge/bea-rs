---
id: yf2
title: 'MCP: archive_task and list_archive tools'
status: done
priority: P2
created: 2026-03-16T21:55:46.173234502Z
updated: 2026-05-31T07:39:24.509080035Z
tags:
- feature
- archive
depends_on:
- 9ra
parent: 6ax
---

Add MCP tools for archive support:

- `archive_task(id)` — move a task to the archive
- `list_archive(limit?)` — return archived tasks, most recent first

Update `prune_tasks` tool to use archive instead of delete, or deprecate it.