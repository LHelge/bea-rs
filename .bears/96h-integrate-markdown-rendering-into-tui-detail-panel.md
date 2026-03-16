---
id: 96h
title: Integrate markdown rendering into TUI detail panel
status: open
priority: P2
created: 2026-03-16T21:55:59.400668115Z
updated: 2026-03-16T21:56:07.741953963Z
tags:
- feature
- tui
depends_on:
- s8r
parent: 7uv
---

Replace the plain-text body rendering in the TUI detail panel with the markdown renderer.

- Use the converter to generate styled Lines
- Ensure scrolling still works with variable-height rendered content
- Test with real task bodies containing mixed markdown elements