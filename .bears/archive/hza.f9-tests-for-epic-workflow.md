---
id: hza.f9
title: Tests for epic workflow
status: done
priority: P1
created: "2026-03-16T21:40:22.229038336Z"
updated: "2026-03-16T23:15:49.613700424Z"
tags:
  - feature
  - epic
  - test
depends_on:
  - hza.95
  - hza.jf
  - hza.b6
  - hza.t8
  - hza.bq
---

Add comprehensive tests for the epic workflow:

Unit tests:
- TaskType serde (serialize/deserialize, default handling)
- Epic progress computation
- Ready excludes epics
- Epic filter on ready/list
- Auto-close logic

Integration tests:
- Create epic + children via CLI
- `bea epics` output
- Complete all children -> epic auto-closes
- `bea ready` never shows epics
- `bea ready --epic <id>` filters correctly