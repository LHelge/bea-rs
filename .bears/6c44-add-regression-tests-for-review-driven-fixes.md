---
id: 6c44
title: Add regression tests for review-driven fixes
status: open
priority: P1
created: 2026-03-16T10:37:28.288294Z
updated: 2026-03-16T10:48:26.234950Z
tags:
- code-review
- testing
depends_on:
- 22b4
- ff18
- 612b
- a4a6
- e5bd
- 482d
- '0e02'
- '6133'
- 5b56
- 6d54
- 57a2
- bc2f
---

## Context

This is the verification umbrella task for all code-review follow-up fixes/refactors.

## Scope

- Add or update tests that lock in behavior for each completed dependency task.
- Ensure coverage across unit, integration, and MCP-facing behavior where relevant.
- Verify no regressions in existing command/tool behavior.

## Acceptance Criteria

- Each dependency task has at least one corresponding regression assertion.
- `cargo fmt`, `cargo clippy`, and `cargo test` pass cleanly.
- New tests are deterministic and platform-agnostic.

## Implementation Checklist

- [ ] Add a regression case for each dependency task outcome.
- [ ] Ensure CLI + MCP coverage where behavior overlaps.
- [ ] Run full verification (`fmt`, `clippy`, `test`).
- [ ] Remove flaky/platform-specific test patterns.
