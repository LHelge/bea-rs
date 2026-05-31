---
id: sy3
title: 'init scaffolding: end-to-end tests + docs'
status: done
priority: P3
created: 2026-05-30T22:25:36.002831264Z
updated: 2026-05-31T08:01:00.700959998Z
tags:
- feature
- init
- test
- docs
depends_on:
- 2k4
- t6h
parent: j6m
---

Depends on the harness tasks (2k4, t6h). Cross-cutting acceptance gate; per-harness tests live with their tasks.

Tests (CLI integration in a temp dir):
- Each flag scaffolds exactly its expected files; flags combine (`bea init --claude --copilot`).
- `.mcp.json` merge preserves a pre-existing unrelated server and is idempotent across re-runs.
- Works both on a fresh dir and an already-initialized one.
- Scaffolded `.mcp.json` uses `bea mcp` (not `cargo run`).
- `cargo package --list` includes all include_str! template sources (guards the publish-time packaging gotcha).

Docs:
- Update CLAUDE.md + README with the new `init` flags and what each scaffolds. Coordinate with 5kx (CLAUDE.md refresh) and 4ag (archive docs).