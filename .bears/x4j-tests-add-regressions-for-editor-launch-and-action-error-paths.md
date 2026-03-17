---
id: x4j
title: 'Tests: add regressions for editor launch and action-error paths'
status: done
priority: P1
created: 2026-03-17T10:14:34.674201Z
updated: 2026-03-17T11:06:50.092694Z
tags:
- tui
- test
- quality
parent: vbj
---

Add tests that fail on current behavior and pass after fixes.

## Why
The review identified high-risk gaps around editor launching and action-level error handling. These need test coverage before implementation changes.

## Scope
- Add focused tests for TUI paths that call editor launch logic.
- Add tests for non-fatal action errors in the run loop path (edit/create/status/reload failures should not crash TUI once fixed).
- Add or adjust tests for selection/reload preservation where relevant.

## Acceptance Criteria
- Failing tests exist first for current incorrect behavior.
- Tests pass after `9ad` and `88c` are complete.
- No existing TUI tests regress.
