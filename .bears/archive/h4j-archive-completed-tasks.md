---
id: h4j
title: Archive completed tasks
type: epic
status: done
priority: P1
created: 2026-05-30T22:13:17.174347401Z
updated: 2026-05-31T08:01:01.203074385Z
tags:
- feature
- archive
---

Move terminal (done/cancelled) tasks out of the active working set into `.bears/archive/`, hiding them from normal commands/tools while keeping them recoverable. Headline feature for the next version.

Decided design (move + invariant):
- Physically move archived files to `.bears/archive/*.md`. `load_all` already reads only top-level `.md` and skips subdirectories, so archived tasks leave the active set for free.
- Invariant: a task is archivable only if it is done/cancelled AND no ACTIVE task depends on it. This guarantees no active task ever references an archived one, so the dependency graph / `ready` / `effective_priority` need no changes and can never see dangling edges. Creating/adding a dep on an archived id already fails as "unknown" (service.rs:30-37), so the invariant is self-maintaining.
- Cascade direction: consumer before supplier. Targeted `bea archive <id>` archives the task and cascades through its now-settled dependency closure; bare `bea archive` sweeps everything currently archivable. Epics and their done children archive together.
- Restore is the mirror: `bea restore <id>` moves back and cascades through depends_on (and parent) so a restored task has no missing deps.
- `prune` is unchanged (hard delete); archive is the soft, recoverable option.
- New-ID generation must also avoid archived IDs so restore can never collide.

Layering (see child tasks + their dependencies): store primitives -> service logic -> CLI + MCP -> end-to-end tests + docs.