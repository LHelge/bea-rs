---
id: qpr
title: Validate grapheme-safe prompt input behavior
status: done
priority: P3
created: 2026-03-17T10:17:47.769144Z
updated: 2026-03-17T12:15:39.353160Z
tags:
- tui
- ux
- polish
parent: vbj
---

Validate and, if needed, improve text prompt handling for multi-byte and grapheme-rich input.

## Problem
Backspace/edit behavior can be surprising for some Unicode/grapheme inputs in create/filter prompts.

## Scope
- Reproduce behavior with accented characters, emoji, and composed graphemes.
- Decide whether to keep current behavior (documented) or improve it.
- If improved, implement safely and add tests for regression coverage.

## Acceptance Criteria
- Prompt editing behavior is explicitly validated.
- Outcome is either improved handling with tests, or clearly documented limitations.
