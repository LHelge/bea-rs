---
id: s8r
title: Implement markdown-to-spans converter
status: open
priority: P2
created: 2026-03-16T21:55:59.394553350Z
updated: 2026-03-16T21:56:07.740325732Z
tags:
- feature
- tui
depends_on:
- dj7
parent: 7uv
---

Implement the markdown-to-ratatui-spans converter.

Supported elements:
- `# Headers` — bold, sized
- `**bold**` and `*italic*`
- `- Bullet lists` and `1. Numbered lists`
- `` `inline code` `` and fenced code blocks — dimmed/colored
- `[links](url)` — show text with URL

Output: Vec<Line> suitable for a ratatui Paragraph widget.