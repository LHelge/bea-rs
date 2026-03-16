---
id: ff18
title: Validate dependency IDs on create/update and in ready
status: open
priority: P0
created: 2026-03-16T10:37:28.216576Z
updated: 2026-03-16T10:37:28.216576Z
tags:
- code-review
- validation
- graph
---

## Context

The review identified that missing dependency IDs can be treated as satisfied in `ready` computation, and create/update paths may accept unknown IDs.

## Scope

- Validate `depends_on` IDs when creating and updating tasks (CLI and MCP).
- Define a clear policy for missing dependencies in graph readiness logic.
- Prefer integrity-first behavior: unknown dependency should block readiness.
- Return clear errors/messages identifying unknown IDs.

## Acceptance Criteria

- Creating or updating with unknown dependency IDs fails with a clear error.
- `bea ready` does not treat missing dependencies as satisfied.
- MCP tool responses are consistent with CLI behavior.
- Tests cover valid dependencies, unknown IDs, and deleted dependency cases.

## Implementation Checklist

- [ ] Implement shared dependency ID validation helper.
- [ ] Apply validation in create/update flows (CLI + MCP).
- [ ] Update readiness logic for missing dependencies.
- [ ] Add tests for unknown and deleted dependency IDs.
