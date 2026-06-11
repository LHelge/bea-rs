---
id: zf9
title: Align TUI delete operation with scope docs
status: done
priority: P3
created: 2026-03-17T10:17:47.757739Z
updated: 2026-03-17T12:11:25.123288Z
tags:
- tui
- quality
- ux
parent: vbj
---

Close the mismatch between documented TUI operations and implemented behavior.

## Problem
TUI scope notes mention delete, but current TUI actions/keys do not expose delete behavior.

## Scope
- Decide and implement one of:
  - add delete action/keypath in TUI, with safeguards, or
  - update docs/epic notes to clearly state delete is out of scope.
- Ensure behavior is consistent across docs and runtime.

## Acceptance Criteria
- No ambiguity remains about delete support in TUI.
- If implemented, delete is tested and discoverable in hints.
- If deferred, docs explicitly reflect current behavior.
