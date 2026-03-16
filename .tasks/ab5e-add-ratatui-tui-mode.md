---
id: ab5e
title: Add ratatui TUI mode
status: open
priority: P2
created: 2026-03-16T09:52:41.490872Z
updated: 2026-03-16T13:50:15.311907Z
tags:
- feature
- tui
---

Add a terminal UI mode via `bea tui` using ratatui. Provides an interactive interface for browsing, filtering, and managing tasks without memorizing CLI commands.

The TUI shall have a treeview, similar to `bea graph` to the left and a detailview of the task to the right. At the bottom there shall be a list of command so perform on the selected task:
- Status: Opens up a modal status selector
- Edit: Open in editor, similar to `bea edit <id>`
- Delete
