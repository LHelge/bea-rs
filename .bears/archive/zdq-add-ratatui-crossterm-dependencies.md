---
id: zdq
title: Add ratatui + crossterm dependencies
status: done
priority: P1
created: 2026-03-16T21:49:49.043185394Z
updated: 2026-03-17T09:29:37.630641Z
tags:
- feature
- tui
parent: und
---

Add ratatui and crossterm as dependencies.

```bash
cargo add ratatui crossterm
```

These are the standard pairing for Rust TUI apps — ratatui for widgets/layout, crossterm for terminal backend and event handling.