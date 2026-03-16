---
id: 5px
title: Update store to handle type field
status: open
priority: P1
created: 2026-03-16T21:40:22.205657124Z
updated: 2026-03-16T21:42:57.367505038Z
tags:
- feature
- epic
depends_on:
- rk4
parent: hza
---

Ensure store correctly reads and writes the `type` field in YAML frontmatter.

- Existing files without `type` should default to `task` (no migration needed)
- New files should include `type` in frontmatter
- Add store-level tests for round-tripping epic files