---
id: 9fcf
title: 'Prefix ID: wire into CLI and MCP commands'
status: done
priority: P1
created: 2026-03-16T09:53:32.087232Z
updated: 2026-03-16T20:59:38.274490139Z
tags:
- feature
- ux
depends_on:
- 05fd
- 6d54
parent: a259
---

Update all CLI commands that accept a task ID to use resolve_prefix instead of exact lookup. This includes show, start, complete, cancel, dep add/remove, delete, update, etc. Also update the MCP tool handlers.