---
id: gz9
title: 'Action: create task with title prompt'
status: open
priority: P1
created: 2026-03-16T21:49:49.069018032Z
updated: 2026-03-16T21:49:58.172237636Z
tags:
- feature
- tui
depends_on:
- 7k7
parent: und
---

Implement the create action in the TUI.

- Press `c` to start creating a new task
- Show an inline input prompt for the task title
- On confirm: create the task via service layer, then open in $EDITOR
- On cancel (Esc): abort creation
- Refresh the task list after creation