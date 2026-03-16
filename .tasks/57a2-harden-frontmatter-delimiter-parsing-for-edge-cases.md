---
id: 57a2
title: Harden frontmatter delimiter parsing for edge cases
status: open
priority: P2
created: 2026-03-16T10:37:28.281036Z
updated: 2026-03-16T10:37:28.281036Z
tags:
- code-review
- parsing
- robustness
---

## Context

Frontmatter parsing currently relies on simple delimiter string search, which is fragile with some line-ending/content edge cases.

## Scope

- Implement a robust frontmatter parser based on clear line-delimiter boundaries.
- Handle BOM and CRLF safely.
- Keep markdown body extraction behavior predictable.

## Acceptance Criteria

- Valid files with LF/CRLF parse correctly.
- Malformed delimiter structures return clear parse errors.
- Tests cover BOM, CRLF, missing delimiters, and no-body cases.

## Implementation Checklist

- [ ] Replace delimiter scanning with robust line-based parsing.
- [ ] Handle BOM and CRLF normalization explicitly.
- [ ] Preserve current valid-body extraction semantics.
- [ ] Add edge-case parser tests.
