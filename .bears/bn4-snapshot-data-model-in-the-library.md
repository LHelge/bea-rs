---
id: bn4
title: Snapshot data model in the library
status: open
priority: P2
created: "2026-11-20T12:52:25.744172059Z"
updated: "2026-11-20T12:52:25.744172059Z"
tags:
  - snapshot
depends_on:
  - s4t
parent: bjt
---

A `snapshot` module in the core: counts per status/priority, epic progress, ready and review queues, in-progress tasks with assignee/attempts, recent archive entries, and the dependency graph with layer assignment (longest-path layering over the DAG) for layout. Pure data, serialisable, no rendering. Option to include or exclude done/archived.