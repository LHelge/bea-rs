---
id: 22b4
title: Make dependency tree traversal cycle-safe
status: open
priority: P0
created: 2026-03-16T10:37:28.208940Z
updated: 2026-03-16T10:37:28.208940Z
tags:
- code-review
- graph
- robustness
---

## Context

The review found that dependency tree building can recurse indefinitely when task files contain a cycle (for example from manual edits or corrupted state).

## Scope

- Make dependency traversal cycle-safe in graph/tree rendering paths.
- Prevent stack overflows by tracking visited nodes (and/or recursion stack) during traversal.
- Decide expected behavior on cycle detection:
	- return an error, or
	- emit a cycle marker node in output.
- Keep CLI and MCP behavior consistent.

## Acceptance Criteria

- `bea dep tree <id>` never hangs or crashes on cyclic data.
- `bea graph` never hangs or crashes on cyclic data.
- Cycle behavior is explicit and tested.
- Add unit tests covering direct and transitive cycles in traversal.

## Implementation Checklist

- [ ] Reproduce current failure mode with a cyclic fixture.
- [ ] Add cycle detection/guard to dependency traversal.
- [ ] Define and implement user-facing cycle behavior.
- [ ] Add traversal regression tests.
