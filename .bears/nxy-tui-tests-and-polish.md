---
id: nxy
title: TUI tests and polish
status: open
priority: P1
created: 2026-03-16T21:49:49.079626128Z
updated: 2026-03-16T21:50:05.090953570Z
tags:
- feature
- tui
- test
depends_on:
- 3wz
- gz9
- y4p
- 5hd
- pge
parent: und
---

Polish and test the TUI feature.

- Manual testing of all keyboard workflows
- Edge cases: empty task list, very long titles, many tasks
- Ensure clean terminal restore on panic/error
- Verify $EDITOR suspend/resume works correctly