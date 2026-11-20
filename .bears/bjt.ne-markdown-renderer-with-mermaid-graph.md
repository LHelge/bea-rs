---
id: bjt.ne
title: Markdown renderer with Mermaid graph
status: done
priority: P2
created: "2026-11-20T12:52:25.748352093Z"
updated: "2026-11-20T20:47:07.163773944Z"
tags:
  - snapshot
depends_on:
  - bjt.zz
attempts: 1
---

Summary tables plus a ```mermaid flowchart of the open dependency graph, with epics as subgraphs and status as node classes. Must render in GitHub job summaries and PR comments. Bound the graph size (fall back to per-epic graphs or a note when too large). Escape titles safely.