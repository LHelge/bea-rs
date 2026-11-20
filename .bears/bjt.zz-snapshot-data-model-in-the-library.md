---
id: bjt.zz
title: Snapshot data model in the library
status: done
priority: P2
created: "2026-11-20T12:52:25.744172059Z"
updated: "2026-11-20T20:37:31.072538266Z"
tags:
  - snapshot
depends_on:
  - 8ph.fc
attempts: 1
---

A `snapshot` module in the core: counts per status/priority, epic progress, ready and review queues, in-progress tasks with assignee/attempts, recent archive entries, and the dependency graph with layer assignment (longest-path layering over the DAG) for layout. Pure data, serialisable, no rendering. Option to include or exclude done/archived.