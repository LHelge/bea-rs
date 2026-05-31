---
id: z54
title: Archivability rule + archive/restore logic in service.rs
status: done
priority: P1
created: 2026-05-30T22:13:31.727218077Z
updated: 2026-05-31T07:25:41.446904794Z
tags:
- feature
- archive
- service
depends_on:
- jcf
parent: h4j
---

Depends on the store layer (jcf). src/service.rs:

- `is_archivable(task, tasks)`: status is Done|Cancelled AND no ACTIVE task depends on it (use the reverse graph). Epics: handle together with their children.
- `archive_task(base, tasks, id_or_prefix)`: validate archivable; cascade through the task's settled dependency closure (and, for epics, done children); move files to the archive. Reject with a clear error listing the active dependents that block it.
- `archive_all(base, tasks)` (sweep): archive every currently-archivable task; return the archived set.
- `restore_task(base, id_or_prefix)`: move back from archive, cascading through depends_on (and parent epic) so the restored task has no missing deps.
- Wire `create_task` id generation to the active+archived id set (jcf helper) for collision-free restore.
- Archive-aware `get_task` lookup for show/restore (read-only).

Add error variants to error.rs (e.g. `NotArchivable { id, blockers }`, `NotArchived`).
Unit tests: archivable rule (with/without active dependents), targeted cascade, sweep, restore cascade, epic+children, id-collision avoidance.