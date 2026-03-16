---
id: und
title: TUI support
status: open
priority: P1
created: 2026-03-16T21:49:09.287614965Z
updated: 2026-03-16T21:49:09.287614965Z
tags:
- feature
- tui
---

Add a TUI mode to bears using ratatui.

## Layout
- Left panel: scrollable task list with selection
- Right panel: detail view showing dependency branch, frontmatter fields, and body
- Bottom bar: keyboard shortcut hints

## Operations
- Browse tasks with keyboard navigation
- Edit: open in $EDITOR (like `bea edit`)
- Create: prompt for title, create task, open in editor
- Status: modal to change task status
- Delete, filtering, sorting

## Technical
- Use ratatui + crossterm
- Wire up as `bea tui` command
- Reuse existing service layer for all operations