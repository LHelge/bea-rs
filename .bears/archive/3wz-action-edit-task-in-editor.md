---
id: 3wz
title: 'Action: edit task in $EDITOR'
status: done
priority: P1
created: 2026-03-16T21:49:49.066570214Z
updated: 2026-03-17T09:44:18.914196Z
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