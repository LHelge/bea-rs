---
id: b87
title: Expose plan_epic as an MCP tool
status: done
priority: P2
created: 2026-05-30T22:01:22.610218163Z
updated: 2026-05-31T06:59:30.008117797Z
tags:
- mcp
- api
- epic
parent: 2kx
---

There is no MCP equivalent of CLI `show <epic> --plan`. `service::plan_epic` (src/service.rs:309) returns an epic's children in topological execution order, but it's only reachable from the CLI — so an agent working an epic can't fetch the ordered plan.

Add an MCP tool, e.g. `plan_epic { id }`, that calls `service::plan_epic` and returns the ordered child task summaries (id, title, priority, status, depends_on) consistent with the other tools' output shape. Mirror the CLI behavior (error if the id is not an epic). Add a tool test covering a linear chain and independent children.