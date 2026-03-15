---
id: '8413'
title: Add bea version command
status: done
priority: P2
created: 2026-03-15T23:40:34.387396890Z
updated: 2026-03-15T23:44:18.013613306Z
tags:
- cli
---

Add a `bea version` command that outputs the current version from Cargo.toml.

Display format:
```
bea 0.2.0
```

With `--json`:
```json
{"version": "0.2.0"}
```

Use `env!("CARGO_PKG_VERSION")` — no build script needed.