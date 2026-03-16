---
id: 6ra
title: Tests for archive feature
status: open
priority: P2
created: 2026-03-16T21:55:46.176963309Z
updated: 2026-03-16T21:56:07.737427712Z
tags:
- feature
- archive
- test
depends_on:
- yf2
parent: 6ax
---

Tests for the archive feature:

Unit tests:
- Store: move file to archive, load from archive
- Service: archive single task, archive all done, list archive

Integration tests:
- `bea archive` moves done tasks
- `bea archive <id>` moves specific task
- Archived tasks don't appear in `bea list` or `bea ready`
- `bea log` shows archived tasks in reverse chronological order