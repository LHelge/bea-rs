---
id: 2j2
title: 'TUI app shell: event loop and terminal setup'
status: done
priority: P1
created: 2026-03-16T21:49:49.049228511Z
updated: 2026-03-17T09:32:08.887318Z
tags:
- feature
- tui
depends_on:
- zdq
parent: und
---

Create the basic TUI application shell in a new `src/tui.rs` module.

- Terminal setup: enter raw mode, enable alternate screen, create crossterm backend
- Terminal teardown: restore on exit (including on panic)
- Main event loop: poll for crossterm events, dispatch to handlers
- Basic App struct holding state (task list, selected index, etc.)
- Quit on `q` or Ctrl+C
- Wire into main.rs module structure (but don't add CLI command yet)