---
id: 6ax
title: Archive and log
type: epic
status: open
priority: P2
created: 2026-03-16T21:55:29.820396424Z
updated: 2026-03-16T21:55:29.820396424Z
tags:
- feature
- archive
---

Replace `bea prune` with an archive system. Instead of deleting completed/cancelled tasks, move them to `.bears/archive/`.

## Design
- `bea archive` moves done/cancelled tasks to `.bears/archive/`
- `bea archive <id>` archives a specific task
- `bea log` displays archived tasks as a chronological log (most recent first)
- Archived tasks no longer appear in `list`, `ready`, or graph queries
- `bea log` supports `--json` flag
- MCP: `archive_task(id)`, `list_archive(limit?)` tools
- Deprecate/remove `bea prune` in favor of archive