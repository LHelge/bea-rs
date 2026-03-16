---
id: acr
title: 'CLI: bea archive and bea log commands'
status: open
priority: P2
created: 2026-03-16T21:55:46.171040468Z
updated: 2026-03-16T21:56:07.730995114Z
tags:
- feature
- archive
depends_on:
- 9ra
parent: 6ax
---

Add CLI commands for archiving:

- `bea archive` — archive all done/cancelled tasks
- `bea archive <id>` — archive a specific task
- `bea log` — display archived tasks as a chronological log
- `bea log --json` — JSON output
- Remove or deprecate `bea prune` (maybe keep as alias initially)