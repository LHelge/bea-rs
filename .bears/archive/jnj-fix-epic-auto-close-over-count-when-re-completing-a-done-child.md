---
id: jnj
title: Fix epic auto-close over-count when re-completing a done child
status: done
priority: P2
created: 2026-05-30T21:38:21.772119364Z
updated: 2026-05-31T06:39:07.888819969Z
tags:
- bug
- epic
parent: nya
---

`src/service.rs:156-163`:

```rust
let progress = epic_progress(tasks, parent_id);
if progress.done + 1 >= progress.total {
```

The `+1` assumes `t` was not already done. Re-completing an already-done child (`bea done` on a done task, or `complete_task` twice) makes the in-memory map already count it in `done`, so `done + 1` overshoots and the epic auto-closes while other children are still open.

Fix: compute "are all children done, treating `t` as done" directly instead of the `+1` shortcut. Add a regression test for re-completing a done child.