---
id: y4p
title: 'Action: status change modal'
status: done
priority: P1
created: 2026-03-16T21:49:49.071753738Z
updated: 2026-03-17T09:44:18.942131Z
tags:
- feature
- tui
depends_on:
- 7k7
parent: und
---

Implement a status change modal in the TUI.

- Press `s` to open a modal/popup listing available statuses
- Navigate with j/k or arrow keys, confirm with Enter
- Update the task status via service layer
- Close modal and refresh task list/detail view
- Esc to cancel