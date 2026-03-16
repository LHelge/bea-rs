---
id: de5b
title: 'Epic: CLI commands'
status: open
priority: P2
created: 2026-03-16T10:17:16.009636Z
updated: 2026-03-16T10:17:16.009636Z
tags:
- feature
- cli
depends_on:
- aa30
parent: f2c5
---

Add `bea epic` subcommand group:
- `bea epic create <title> [--priority] [--tags]`: create a task with type=epic
- `bea epic list`: list all epics with progress bars/counts (e.g. `[3/7] 43%`)
- `bea epic show <id>`: show epic details plus a table of child tasks with statuses

Also update `bea list` and `bea graph` to visually distinguish epics (e.g. prefix or styling).