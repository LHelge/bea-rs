---
id: 62b
title: 'TUI: scrollable detail view with Tab focus toggle'
status: done
priority: P2
created: 2026-03-17T11:18:27.951292Z
updated: 2026-03-17T13:42:03.095656Z
tags:
- tui
- feature
- ux
parent: und
---

The detail panel (right side) has no scroll support — long task bodies are clipped.

## Scope
- Add a focus concept: Tab toggles active pane between task list (left) and detail view (right).
- Visual indicator showing which pane is focused (e.g. highlighted border).
- When detail view is focused, j/k (or Up/Down) scrolls the detail content.
- When task list is focused, j/k navigates tasks as today.
- Scroll state resets when a different task is selected.

## Acceptance Criteria
- Tab switches focus between list and detail panes.
- Focused pane has a distinct border style.
- Detail view scrolls vertically when focused and content overflows.
- Existing keybindings (e/c/s/q/etc.) continue working in both focus states.
