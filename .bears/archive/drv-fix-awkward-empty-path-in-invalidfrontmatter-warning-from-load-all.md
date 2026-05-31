---
id: drv
title: Fix awkward empty-path in InvalidFrontmatter warning from load_all
status: done
priority: P3
created: 2026-05-30T21:39:16.592642495Z
updated: 2026-05-30T23:04:53.526551100Z
tags:
- cleanup
- error-handling
parent: nya
---

`parse_task` sets `InvalidFrontmatter { path: "".into() }` (`src/task.rs:339-342, 358-361`). `load_one` patches the real path back in, but `load_all` (`src/store.rs:65`) doesn't, so its warning reads `warning: skipping /real/path.md: invalid frontmatter in : <reason>` — the empty `in ` is awkward. Either thread the real path through in `load_all` (like `load_one` does) or drop `path` from the message when empty.