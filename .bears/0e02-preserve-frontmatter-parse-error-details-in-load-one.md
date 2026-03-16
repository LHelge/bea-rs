---
id: '0e02'
title: Preserve frontmatter parse error details in load_one
status: done
priority: P2
created: 2026-03-16T10:37:28.245805Z
updated: 2026-03-16T19:25:28.863320Z
tags:
- code-review
- store
- error-handling
---

## Context

Single-task load currently collapses parse failures into a generic message, losing useful YAML/parse diagnostics.

## Scope

- Preserve original parse error reason when mapping to domain error.
- Keep file path included in all frontmatter errors.
- Ensure CLI and MCP surface actionable messages.

## Acceptance Criteria

- `load_one` error includes specific parse reason and file path.
- Error text is human-readable and stable for tests.
- Tests cover malformed YAML and missing delimiters.

## Implementation Checklist

- [ ] Preserve parser error cause in error mapping.
- [ ] Ensure path + reason are both surfaced consistently.
- [ ] Update user-facing error formatting if needed.
- [ ] Add/adjust tests for malformed frontmatter cases.
