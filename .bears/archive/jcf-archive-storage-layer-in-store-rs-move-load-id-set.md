---
id: jcf
title: Archive storage layer in store.rs (move, load, id-set)
status: done
priority: P1
created: 2026-05-30T22:13:23.625119247Z
updated: 2026-05-30T22:58:21.725495673Z
tags:
- feature
- archive
- store
parent: h4j
---

Foundation for the archive feature. src/store.rs:

- `archive_dir(base)` -> `.bears/archive/`, created on demand.
- Move helpers: active <-> archive (move the `{id}-{slug}.md` file between `.bears/` and `.bears/archive/`).
- `load_archived(base)` -> HashMap of archived tasks, mirroring `load_all` but reading the archive dir.
- `find_archived_path` + archive-aware prefix resolution (for show/restore).
- A helper returning the set of ALL known IDs (active + archived) so new IDs never collide with archived ones.

Verify/lock in that `load_all` ignores the `archive/` subdir (it filters on the `.md` extension, so a directory is skipped) with a test.

Tests: move round-trip, load_archived, load_all ignores archive dir, all-ids set includes archived.