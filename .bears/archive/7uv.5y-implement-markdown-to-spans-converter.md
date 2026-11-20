---
id: 7uv.5y
title: Implement markdown-to-spans converter
status: done
priority: P2
created: "2026-03-16T21:55:59.394553350Z"
updated: "2026-03-17T20:55:11.801041359Z"
tags:
  - feature
  - tui
depends_on:
  - 7uv.ah
---

Implement the markdown-to-ratatui-spans converter.

Supported elements:
- `# Headers` — bold, sized
- `**bold**` and `*italic*`
- `- Bullet lists` and `1. Numbered lists`
- `` `inline code` `` and fenced code blocks — dimmed/colored
- `[links](url)` — show text with URL

Output: Vec<Line> suitable for a ratatui Paragraph widget.