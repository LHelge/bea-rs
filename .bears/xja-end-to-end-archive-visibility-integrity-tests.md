---
id: xja
title: End-to-end archive visibility & integrity tests
status: done
priority: P2
created: 2026-05-30T22:13:50.514884661Z
updated: 2026-05-31T08:01:00.281146358Z
tags:
- feature
- archive
- test
depends_on:
- awg
- 5wj
parent: h4j
---

Cross-cutting acceptance gate (unit tests live with their own tasks). Depends on the CLI (awg) and MCP (5wj) tasks.

Cover:
- Archived tasks hidden from list, ready, search, graph, epics (CLI + MCP).
- Targeted archive cascades through settled dependencies; refuses when an active task still depends on the target, and the error names the blocker.
- Sweep archives only currently-archivable tasks.
- Restore (with cascade) makes a task ready/resolvable again.
- `prune` still hard-deletes (unchanged) and never touches the archive.
- New task IDs never collide with archived IDs.
- A dependency cannot be added onto an archived id (stays "unknown"), preserving the invariant.