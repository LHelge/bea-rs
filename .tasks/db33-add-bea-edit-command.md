---
id: db33
title: Add bea edit command
status: open
priority: P2
created: 2026-03-15T20:47:44.260527967Z
updated: 2026-03-15T20:47:44.260527967Z
tags:
- cli
---

Open a task's .md file in $EDITOR (fall back to $VISUAL, then vi). After the editor exits, re-parse the file to validate frontmatter and report any errors. Similar to 'git commit' flow.