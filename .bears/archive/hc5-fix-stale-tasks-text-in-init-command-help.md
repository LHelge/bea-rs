---
id: hc5
title: Fix stale `.tasks/` text in `init` command help
status: done
priority: P2
created: 2026-05-30T21:38:31.475867599Z
updated: 2026-05-30T22:55:53.072422764Z
tags:
- bug
- docs
- cli
parent: nya
---

`src/cli/args.rs:23` documents the `Init` command as:

```rust
/// Initialize a new .tasks/ directory
```

The tool creates `.bears/` (and `tests/cli.rs::test_init_creates_bears_dir` asserts it is *not* `.tasks/`). Update the help text to say `.bears/`. Trivial, user-facing.