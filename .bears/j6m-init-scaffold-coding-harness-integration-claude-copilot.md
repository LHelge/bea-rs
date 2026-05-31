---
id: j6m
title: 'init: scaffold coding-harness integration (--claude/--copilot)'
type: epic
status: done
priority: P2
created: 2026-05-30T22:24:55.593187151Z
updated: 2026-05-31T08:02:25.637716728Z
tags:
- feature
- init
- dx
---

Extend `bea init` with per-harness flags that scaffold the files a coding harness needs to work with bears. e.g. `bea init --claude` writes `.mcp.json` + the bears skill/agent. v1 harnesses: Claude and Copilot. (Codex deferred — parked in cancelled task 9m8.)

Decided design:
- Per-harness flags on `init` (`--claude`, `--copilot`), repeatable/combinable; table-driven so more harnesses (incl. Codex later) are easy to add. Still creates `.bears/`+`.bears.yml` and works on an already-initialized project (adds/refreshes harness files).
- Embed templates with `include_str!` from canonical sources. Claude content exists (.claude/skills/bears-planning/{SKILL.md,references/cli-fallback.md}, .claude/agents/planner.md); Copilot content exists (.github/skills/bears-planning/..., .github/agents/planner.agent.md). NOTE the repo own .mcp.json uses `cargo run -- mcp` (dev) — the SCAFFOLDED one must use the installed binary: {"mcpServers":{"bears":{"command":"bea","args":["mcp"]}}}.
- Non-destructive: refresh bears-owned skill/agent files; MERGE `.mcp.json` (add/replace only the "bears" key, preserve other servers).
- Packaging caveat: include_str! sources must ship in the published crate — verify `cargo package --list`, relocate under a packaged dir if needed.

Layering: core framework -> per-harness (Claude/Copilot) -> tests+docs.