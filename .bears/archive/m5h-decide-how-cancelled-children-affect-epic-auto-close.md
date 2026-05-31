---
id: m5h
title: Decide how cancelled children affect epic auto-close
status: done
priority: P2
created: 2026-05-30T21:38:26.777516212Z
updated: 2026-05-31T06:39:08.347733896Z
tags:
- bug
- epic
- design
parent: nya
---

`epic_progress` counts every child in `total` but only `Done` in `done`, so a cancelled child counts against the epic forever: `[done, cancelled]` reaches `done=1, total=2` and never auto-closes. Also, cancelling the last open child doesn't trigger the check at all — auto-close only runs when the new status is `Done` (`src/service.rs:149-164`).

CLAUDE.md says "when all children are completed". Decide whether cancelled counts as completed, then make `epic_progress` / auto-close consistent (likely: treat cancelled children as not-blocking, and re-check on cancel as well as done). Add tests for the chosen semantics.