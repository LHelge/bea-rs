---
id: und.fh
title: Keyboard navigation (j/k, tab, esc, quit)
status: done
priority: P1
created: "2026-03-16T21:49:49.065397026Z"
updated: "2026-03-17T09:37:57.477951Z"
tags:
  - feature
  - tui
depends_on:
  - und.jd
  - und.6f
---

Implement keyboard navigation across the TUI.

- j/k or Up/Down: move selection in task list
- Tab: cycle focus between panels (if applicable)
- Enter: expand/collapse or trigger default action
- Esc: close modals, cancel operations
- q: quit