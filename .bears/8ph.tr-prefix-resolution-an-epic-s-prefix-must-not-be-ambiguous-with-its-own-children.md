---
id: 8ph.tr
title: "Prefix resolution: an epic's prefix must not be ambiguous with its own children"
status: done
priority: P1
created: "2026-11-20T13:59:52.574418986Z"
updated: "2026-11-20T14:11:02.619247371Z"
tags:
  - ids
depends_on:
  - 8ph.fc
attempts: 1
---

With hierarchical IDs, `store::resolve_prefix` uses `starts_with`, so once an epic has children a short prefix of the epic (`bea show 8p`, `--parent 8p`) is ambiguous with `8ph.xx`. Exact IDs still work. When every match is one task or a descendant of it, resolve to that shallowest task; keep the ambiguity error for genuinely different tasks. Also decide and test how a prefix that ends inside a child segment (`8ph.c`) resolves. Applies to active and archived resolution, CLI, MCP and TUI alike (fix in the library). Tests in store.rs and tests/cli.rs.