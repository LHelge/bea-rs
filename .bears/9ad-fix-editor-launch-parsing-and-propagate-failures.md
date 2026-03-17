---
id: 9ad
title: Fix editor launch parsing and propagate failures
status: done
priority: P1
created: 2026-03-17T10:14:34.679982Z
updated: 2026-03-17T11:14:49.699903Z
tags:
- tui
- bug
- quality
depends_on:
- x4j
parent: vbj
---

Fix both high-severity editor issues identified in review.

## Problems To Fix
- Editor command parsing currently uses whitespace splitting and does not handle quoted args or paths with spaces robustly.
- Editor launch failures or non-zero exit can be ignored, which makes edit/create appear successful when they are not.

## Scope
- Replace fragile editor parsing with robust command parsing.
- Return actionable errors when editor command cannot be executed.
- Treat non-success editor exit as failure (or explicit handled warning) instead of silent success.
- Keep terminal state restoration safe when editor launch fails.

## Acceptance Criteria
- Setting tricky `EDITOR` values (quoted args, spaces) works or fails with clear errors.
- Failed editor launch is visible to caller and no longer silently swallowed.
- TUI remains usable after failed editor attempts.
- Tests from `x4j` cover these cases.
