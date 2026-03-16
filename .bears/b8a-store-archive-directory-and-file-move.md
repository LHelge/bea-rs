---
id: b8a
title: 'Store: archive directory and file move'
status: open
priority: P2
created: 2026-03-16T21:55:46.161258953Z
updated: 2026-03-16T21:55:46.161258953Z
tags:
- feature
- archive
parent: 6ax
---

Create `.bears/archive/` directory support in the store layer.

- `store::init` should create `.bears/archive/` alongside `.bears/`
- Add `archive_task()`: move a task file from `.bears/` to `.bears/archive/`
- Add `load_archive()`: load all tasks from `.bears/archive/`
- Archived tasks should not be loaded by `load_all()` (active tasks only)