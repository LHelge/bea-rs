---
id: d7r
title: Cascade epic auto-close through nested epics
status: done
priority: P3
created: 2026-05-30T21:39:26.657799055Z
updated: 2026-05-31T06:39:08.853163181Z
tags:
- bug
- epic
- edge-case
parent: nya
---

Auto-close (`src/service.rs:149-164`) walks exactly one level up and writes the parent directly (not via `set_status`). If epics nest (epic → epic → tasks), closing the last task closes the inner epic but never re-evaluates the outer one. Make auto-close re-check ancestors (e.g., recurse when the closed parent itself has a parent epic). Add a nested-epic test. Coordinate with the auto-close refactor in fkk/jnj/m5h.