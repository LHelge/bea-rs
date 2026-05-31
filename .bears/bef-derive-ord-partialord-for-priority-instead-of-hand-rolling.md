---
id: bef
title: Derive Ord/PartialOrd for Priority instead of hand-rolling
status: done
priority: P3
created: 2026-05-30T21:38:57.031192242Z
updated: 2026-05-30T22:53:33.842675146Z
tags:
- cleanup
- idiomatic
parent: nya
---

`src/task.rs:94-112` manually implements `Ord`/`PartialOrd` for `Priority` via a `rank` closure that reproduces declaration order (P0 < P1 < P2 < P3). Since the variants are already declared in the desired order, `#[derive(PartialOrd, Ord)]` gives identical behavior and removes ~18 lines of boilerplate. Verify `test_priority_ordering` still passes.