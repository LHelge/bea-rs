---
id: und.d4
title: TUI tests and polish
status: done
priority: P1
created: "2026-03-16T21:49:49.079626128Z"
updated: "2026-03-17T09:54:16.469288Z"
tags:
  - feature
  - tui
  - test
depends_on:
  - und.q8
  - und.8h
  - und.qe
  - und.55
  - und.h4
---

Polish and test the TUI feature.

- Manual testing of all keyboard workflows
- Edge cases: empty task list, very long titles, many tasks
- Ensure clean terminal restore on panic/error
- Verify $EDITOR suspend/resume works correctly