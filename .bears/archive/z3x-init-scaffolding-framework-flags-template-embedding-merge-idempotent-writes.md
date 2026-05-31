---
id: z3x
title: 'init scaffolding framework: flags, template embedding, merge/idempotent writes'
status: done
priority: P1
created: 2026-05-30T22:25:05.552342382Z
updated: 2026-05-31T06:44:58.043222047Z
tags:
- feature
- init
parent: j6m
---

Foundation shared by all harnesses.

- Add per-harness flags to the `Init` command (`--claude`, `--copilot`, `--codex`), combinable. Keep plain `bea init` behavior. Allow running on an already-initialized project (add/refresh harness files without erroring).
- Table-driven harness registry: each harness maps to a set of (target relative path -> embedded content) plus its MCP-registration strategy. Adding a harness should be a data change, not new control flow.
- Embed templates via `include_str!`. Decide the source location and VERIFY packaging: run `cargo package --list` to confirm the referenced files ship in the published crate; if dotfile dirs are excluded, relocate template sources under a packaged dir (e.g. `templates/`) or dogfood (generate this repo's own `.claude`/`.github` from the tool).
- Write helpers:
  - Plain file write with parent-dir creation (refresh bears-owned files).
  - JSON merge for `.mcp.json`: parse existing (if any) with serde_json, add/replace only the `bears` server key, preserve other servers, write back pretty. Create fresh if absent.
- `--json` output: list created/updated file paths.

Unit tests: merge into existing .mcp.json preserves other servers; merge is idempotent; fresh-create path.