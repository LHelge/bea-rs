---
id: yrt
title: Deduplicate text-input handlers and prompt rendering
status: done
priority: P2
created: 2026-03-17T10:14:34.697679Z
updated: 2026-03-17T11:28:52.772374Z
tags:
- tui
- refactor
- readability
depends_on:
- d3e
parent: vbj
---

Refactor duplicate create/filter input logic and duplicate prompt rendering.

## Problem
- `handle_create_input_key` and `handle_filter_input_key` are structurally similar.
- `render_create_input` and `render_filter_input` are near-identical.

## Scope
- Introduce shared helpers for text-input editing behavior where practical.
- Introduce shared prompt-render helper (title + input text).
- Keep mode-specific semantics intact (create returns action, filter applies query).

## Acceptance Criteria
- Clear reduction in duplicate code in input handling and rendering.
- No key-behavior regressions in create/filter modes.
- Code is easier to read and maintain.
