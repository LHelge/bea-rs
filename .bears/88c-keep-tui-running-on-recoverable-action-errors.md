---
id: 88c
title: Keep TUI running on recoverable action errors
status: done
priority: P1
created: 2026-03-17T10:14:34.685847Z
updated: 2026-03-17T11:22:19.158651Z
tags:
- tui
- bug
- ux
depends_on:
- x4j
parent: vbj
---

Improve TUI runtime resilience so ordinary operation errors do not terminate the session.

## Problem
The event loop currently uses `?` in action branches, so store/service/editor errors can bubble up and end the TUI.

## Scope
- Handle recoverable errors inside the action dispatch loop.
- Surface error messages in TUI state (status line or equivalent), not by immediate process exit.
- Keep hard failures explicit, but avoid exiting on expected operational errors.

## Acceptance Criteria
- Failing edit/create/status operations do not immediately close TUI.
- User sees a clear error message for the failed action.
- Normal operation can continue after the error.
- Tests from `x4j` validate the behavior.
