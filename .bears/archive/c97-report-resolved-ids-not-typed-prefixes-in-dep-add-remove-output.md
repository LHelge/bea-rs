---
id: c97
title: Report resolved IDs (not typed prefixes) in dep add/remove output
status: done
priority: P3
created: 2026-05-30T21:38:35.938167802Z
updated: 2026-05-30T22:55:53.402691039Z
tags:
- inconsistency
- cli
parent: nya
---

`src/cli/cmd.rs:352` and `:369` print the raw prefixes the user typed:

```rust
println!("[{}] now depends on [{}]", id, depends_on);
```

`bea dep add ab cd` prints `[ab] now depends on [cd]` even though both resolved to full IDs. Use the returned `t.id` and the resolved dependency id so output reflects the actual tasks.