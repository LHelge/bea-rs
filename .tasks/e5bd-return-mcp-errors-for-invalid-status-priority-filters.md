---
id: e5bd
title: Return MCP errors for invalid status/priority filters
status: open
priority: P1
created: 2026-03-16T10:37:28.238294Z
updated: 2026-03-16T10:37:28.238294Z
tags:
- code-review
- mcp
- validation
---

## Context

MCP filter parsing currently uses permissive parsing and silently ignores invalid `status`/`priority` values.

## Scope

- Validate MCP `status` and `priority` filter params strictly.
- Return explicit MCP tool errors for invalid values.
- Keep accepted values aligned with CLI parsing.

## Acceptance Criteria

- Invalid MCP filter input returns `isError=true` with a clear message.
- Valid MCP filters continue to work unchanged.
- Tests cover both valid and invalid parameter cases.

## Implementation Checklist

- [ ] Add strict parsing/validation for filter params.
- [ ] Return explicit tool errors for invalid values.
- [ ] Keep valid filter behavior unchanged.
- [ ] Add tests for accepted and rejected values.
