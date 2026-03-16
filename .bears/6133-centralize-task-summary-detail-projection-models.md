---
id: '6133'
title: Centralize task summary/detail projection models
status: done
priority: P2
created: 2026-03-16T10:37:28.259922Z
updated: 2026-03-16T19:32:57.756616Z
tags:
- code-review
- refactor
- readability
depends_on:
- 482d
---

## Context

Task summary/detail JSON shaping is currently duplicated across layers and commands/tools.

## Scope

- Introduce shared projection structs/functions (`TaskSummary`, `TaskDetail`, etc.).
- Remove duplicate ad-hoc JSON construction where possible.
- Keep output formats backward-compatible unless explicitly changed.

## Acceptance Criteria

- One canonical projection path is used by CLI and MCP.
- Duplicate summary helpers are removed.
- Serialization tests confirm expected fields and values.

## Implementation Checklist

- [ ] Define shared projection structs/functions.
- [ ] Replace ad-hoc JSON shaping in CLI and MCP.
- [ ] Keep output shape backward-compatible.
- [ ] Add serialization and contract tests.
