---
id: 5b56
title: Consolidate common task filtering and sorting helpers
status: open
priority: P2
created: 2026-03-16T10:37:28.266845Z
updated: 2026-03-16T10:38:00.090253Z
tags:
- code-review
- refactor
- duplication
depends_on:
- 482d
---

## Context

Filtering and sorting logic (status, priority, tag, ready ordering) is repeated in several CLI and MCP paths.

## Scope

- Extract shared helpers for common task filtering semantics.
- Extract shared helper for canonical task ordering.
- Reuse helpers in list/ready/search and equivalent MCP tools.

## Acceptance Criteria

- Repeated filter/sort blocks are replaced with shared helpers.
- Ordering behavior remains unchanged and deterministic.
- Tests lock in expected ordering and filter behavior.

## Implementation Checklist

- [ ] Extract shared filtering helpers.
- [ ] Extract shared canonical sorting helper.
- [ ] Apply helpers in CLI and MCP list/ready/search flows.
- [ ] Add targeted tests for ordering/filter parity.
