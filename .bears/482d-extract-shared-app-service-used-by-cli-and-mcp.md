---
id: 482d
title: Extract shared app service used by CLI and MCP
status: done
priority: P1
created: 2026-03-16T10:37:28.252974Z
updated: 2026-03-16T18:38:47.313891Z
tags:
- code-review
- architecture
- refactor
---

## Context

Core business logic is duplicated across CLI and MCP layers (create/update/status/dependencies/search/prune), increasing maintenance cost and drift risk.

## Scope

- Introduce a shared application service module for task operations.
- Keep `store` and `graph` as core dependencies used by the service.
- Make CLI and MCP thin adapters that translate input/output only.
- Migrate incrementally to avoid broad regressions.

## Acceptance Criteria

- Core task operations are implemented once in shared code.
- CLI and MCP paths call shared service methods.
- Behavior remains consistent with existing command/tool contracts.
- Tests are updated or added to verify parity.

## Implementation Checklist

- [ ] Design shared service API for core operations.
- [ ] Migrate one command/tool path end-to-end first.
- [ ] Port remaining duplicated logic incrementally.
- [ ] Remove old duplicate code and update tests.
