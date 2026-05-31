---
id: a3x
title: Remove dead `json` parameter from cli::output helper
status: done
priority: P3
created: 2026-05-30T21:38:49.489953381Z
updated: 2026-05-30T22:55:52.518574069Z
tags:
- cleanup
- cli
parent: nya
---

`src/cli/mod.rs:132-137`:

```rust
fn output<T: Serialize>(value: &T, json: bool) -> Result<()> {
    if json { println!(...); }
    Ok(())
}
```

Every call site passes `true` and only calls it inside an `if json` branch already, so both the parameter and the inner `if` are vestigial. Simplify to an always-print helper and drop the argument at call sites.