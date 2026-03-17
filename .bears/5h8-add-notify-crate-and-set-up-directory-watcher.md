---
id: 5h8
title: Add notify crate and set up directory watcher
status: open
priority: P1
created: 2026-03-17T21:39:04.407057148Z
updated: 2026-03-17T21:39:04.407057148Z
tags:
- tui
parent: ssj
---

Add `notify` (with debouncer) as a dependency.

Set up a watcher on the `.bears/` directory that emits a channel message
on any Create/Modify/Remove event. Use `notify-debouncer-mini` or manual
debounce (~200-500ms) to coalesce rapid writes.

The watcher should run in a background tokio task and send events through
an `mpsc` or `crossterm` user-event channel so the TUI event loop can
pick them up without polling.