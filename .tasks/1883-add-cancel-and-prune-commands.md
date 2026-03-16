---
id: '1883'
title: Add cancel and prune commands
status: open
priority: P1
created: 2026-03-16T09:52:32.418620Z
updated: 2026-03-16T09:52:32.418620Z
tags:
- feature
- cli
---

Add three new CLI commands:

- `bea cancel <id>`: Set task status to cancelled
- `bea prune`: Delete all cancelled tasks
- `bea prune --done`: Delete all cancelled AND done tasks

These should also be exposed as MCP tools.