---
id: dj7
title: Choose markdown rendering approach
status: done
priority: P2
created: 2026-03-16T21:55:59.384688779Z
updated: 2026-03-17T20:52:45.127504749Z
tags:
- feature
- tui
depends_on:
- 98y
parent: 7uv
---

Research and choose a markdown-to-styled-text approach for ratatui.

Options:
- Hand-rolled line-by-line parser (headers, bold, italic, lists, code)
- Small crate like `pulldown-cmark` to parse, then convert events to ratatui Spans
- `tui-markdown` crate if it exists and is maintained

Decide based on: dependency weight, feature coverage, maintainability.