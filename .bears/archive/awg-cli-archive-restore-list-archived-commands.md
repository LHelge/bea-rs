---
id: awg
title: 'CLI: archive / restore / list --archived commands'
status: done
priority: P2
created: 2026-05-30T22:13:38.199839532Z
updated: 2026-05-31T07:42:25.570504117Z
tags:
- feature
- archive
- cli
depends_on:
- z54
parent: h4j
---

Depends on the service layer (z54). src/cli/args.rs + cmd.rs + mod.rs:

- `bea archive [id]`: no id => sweep all archivable; with id => archive that task and cascade. `--json` outputs the archived set.
- `bea restore <id>`: restore from archive (cascade). `--json`.
- `bea list --archived`: list archived tasks (reads the archive dir). Normal list/ready/search/graph/epics stay active-only (already the case since archived aren't loaded).
- `show <id>`: resolve active first, then fall back to the archive (mark output as archived). Mutating commands (start/done/update/dep) stay active-only and should error helpfully on an archived id ("restore it first").

Integration-test the happy paths here; the broad visibility/integrity guarantees live in the dedicated test task.