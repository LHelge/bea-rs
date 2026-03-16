---
id: 7uv
title: Markdown rendering in TUI
status: open
priority: P2
created: 2026-03-16T21:55:29.826406210Z
updated: 2026-03-16T21:55:29.826406210Z
tags:
- feature
- tui
---

Add basic markdown rendering in the TUI detail panel using ratatui styled text.

## Scope
- Headers: bold, different sizes
- Bold / italic text
- Bullet and numbered lists
- Code blocks and inline code (dimmed or different color)
- Links (show URL in parentheses)

Keep it lightweight — no full CommonMark parser, just enough to make task bodies readable. Consider using a small crate or a simple hand-rolled spans converter.