---
id: 9ra
title: 'Service: archive and list_archive operations'
status: done
priority: P2
created: 2026-03-16T21:55:46.168667013Z
updated: 2026-05-31T07:25:41.605937002Z
tags:
- feature
- archive
depends_on:
- b8a
parent: 6ax
---

Add archive operations to the service layer.

- `archive_task(id)`: validate task exists, move to archive
- `archive_done()`: archive all tasks with status done or cancelled
- `list_archive(limit?)`: load and return archived tasks sorted by updated date (most recent first)