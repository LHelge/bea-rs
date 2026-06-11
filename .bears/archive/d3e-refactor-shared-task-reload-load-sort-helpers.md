---
id: d3e
title: Refactor shared task reload/load/sort helpers
status: done
priority: P2
created: 2026-03-17T10:14:34.691625Z
updated: 2026-03-17T11:25:39.230334Z
tags:
- tui
- refactor
- quality
parent: vbj
---

Reduce duplication in TUI runtime data refresh paths.

## Problem
Load/sort/reload logic and async-to-sync bridging are repeated in multiple places (`run`, `reload`, create/status action paths).

## Scope
- Extract shared helper(s) for loading and sorting tasks.
- Consolidate refresh/update flow after mutations.
- Preserve current behavior (selection restoration, ordering, graph rebuild).

## Acceptance Criteria
- TUI runtime has one clear source of truth for load/sort behavior.
- Duplicated blocks are removed without changing output ordering.
- Existing behavior remains stable and tested.
