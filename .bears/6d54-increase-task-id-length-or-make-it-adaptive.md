---
id: 6d54
title: Increase task ID length or make it adaptive
status: open
priority: P2
created: 2026-03-16T10:37:28.274332Z
updated: 2026-03-16T10:46:51.074406Z
tags:
- code-review
- task-model
depends_on:
- 7af8
---

## Context

Current task IDs use 4 hex characters, which is convenient but collision risk grows with repository size.

## Scope

- Choose and implement an updated ID strategy:
	- fixed longer IDs (for example 6 chars), or
	- adaptive length that grows on collision pressure.
- Ensure compatibility with existing tasks and prefix ID resolution.
- Document migration/compatibility behavior.

## Acceptance Criteria

- New ID strategy reduces collision likelihood measurably.
- Existing task files remain readable and operable.
- Unit tests cover generation uniqueness and collision handling.

## Implementation Checklist

- [ ] Choose fixed-length or adaptive ID approach.
- [ ] Update generation logic and collision handling.
- [ ] Confirm compatibility with existing IDs/prefix matching.
- [ ] Add tests and document behavior in README.
