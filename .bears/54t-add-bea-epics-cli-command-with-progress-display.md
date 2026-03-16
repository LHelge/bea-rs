---
id: 54t
title: Add bea epics CLI command with progress display
status: done
priority: P1
created: 2026-03-16T21:40:22.220948494Z
updated: 2026-03-16T22:51:52.066081966Z
tags:
- feature
- epic
depends_on:
- vcq
- dr6
parent: hza
---

Add a new `bea epics` CLI command that lists all epics with:
- Epic title prefixed with "Epic: " in a bright color
- Progress indicator: `[done/total]`
- Priority and status
- Tags

Sort by priority (P0 first), then creation date.