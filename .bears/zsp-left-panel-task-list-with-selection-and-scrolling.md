---
id: zsp
title: 'Left panel: task list with selection and scrolling'
status: done
priority: P1
created: 2026-03-16T21:49:49.052029640Z
updated: 2026-03-17T09:35:05.857839Z
tags:
- feature
- tui
depends_on:
- 2j2
parent: und
---

Implement the left panel showing a scrollable list of tasks.

- Display task ID, priority (colored), status indicator, and title
- Highlight the currently selected task
- Support scrolling when list exceeds panel height
- Load tasks from store on startup
- Sort by priority then creation date (like `bea list`)