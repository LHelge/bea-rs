---
id: bc2f
title: Rename task storage directory from .tasks to .bears
status: open
priority: P1
created: 2026-03-16T10:46:46.748188Z
updated: 2026-03-16T10:46:46.748188Z
tags:
- code-review
- storage
- migration
---

## Context

Task storage currently uses `.tasks`, but we want to standardize on `.bears` as the project data directory.

## Scope

- Rename default task storage directory from `.tasks` to `.bears`.
- Update CLI, MCP, store constants, docs, and tests accordingly.
- Add compatibility/migration behavior for repositories that already have `.tasks`.
- Define precedence if both directories exist.

## Acceptance Criteria

- New repositories use `.bears` by default.
- Existing repositories with `.tasks` still work with a clear migration path.
- Documentation and examples consistently reference `.bears`.
- Test suite covers both fresh and migrated repos.

## Implementation Checklist

- [ ] Update directory constants and path resolution.
- [ ] Implement migration or dual-read compatibility behavior.
- [ ] Update README/tests/fixtures to `.bears`.
- [ ] Add tests for migration and conflict scenarios.