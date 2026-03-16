---
id: 3wz
title: 'Action: edit task in $EDITOR'
status: open
priority: P1
created: 2026-03-16T21:49:49.066570214Z
updated: 2026-03-16T21:49:58.168955813Z
tags:
- feature
- tui
depends_on:
- 7k7
parent: und
---

Implement the edit action in the TUI.

- Press `e` to open the selected task's body in $EDITOR/$VISUAL
- Suspend the TUI (leave alternate screen), launch editor
- On editor exit, reload the task from disk and restore the TUI
- Same behavior as `bea edit <id>`