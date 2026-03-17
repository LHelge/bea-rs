---
id: b22
title: Integrate watcher events into TUI event loop
status: open
priority: P1
created: 2026-03-17T21:39:10.665872097Z
updated: 2026-03-17T21:39:10.665872097Z
tags:
- tui
depends_on:
- 5h8
parent: ssj
---

Integrate the file-change channel into the TUI event loop
(`crossterm::event::poll` / `select!`). When a reload signal arrives:

1. Call `store::load_all` to re-read all task files.
2. Rebuild the `Graph`.
3. Update `App.all_tasks`, `App.task_map`, and `App.graph`.
4. Re-run `apply_filter()` to refresh the visible list.
5. Preserve `last_selected_id` so the cursor doesn't jump.