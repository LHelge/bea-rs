---
id: dy6
title: Break exact ties by ID in ready and list ordering
status: done
priority: P3
created: "2026-11-20T20:40:33.541276774Z"
updated: "2026-11-20T21:59:46.531845173Z"
tags:
  - core
attempts: 1
---

Found during the snapshot epic (bjt.zz). `Graph::ready` and `task::sort_by_priority_owned` sort by (effective) priority then creation date; tasks equal in both come out in `HashMap` iteration order, so `bea ready` / `bea list` (and the MCP lists) are nondeterministic for exact ties. Add the ID as the final tie-break in the core. `snapshot` re-sorts locally with an ID tie-break as a workaround; remove that once the core does it.