---
id: 2k4
title: 'init --claude: emit .mcp.json + bears skill + planner agent'
status: done
priority: P2
created: 2026-05-30T22:25:12.263654591Z
updated: 2026-05-31T07:31:54.794037074Z
tags:
- feature
- init
- claude
depends_on:
- z3x
parent: j6m
---

Depends on the scaffolding framework (z3x). Content already exists in-repo.

`bea init --claude` emits:
- `.mcp.json` — merge in `bears` -> `{command:"bea", args:["mcp"]}` (NOT the repo's dev `cargo run` form).
- `.claude/skills/bears-planning/SKILL.md` and `.claude/skills/bears-planning/references/cli-fallback.md` (include_str! from the canonical files).
- `.claude/agents/planner.md` (include_str!).

Refresh the skill/agent files on re-run; merge (don't clobber) `.mcp.json`.
Integration test: into a temp dir, assert all files written with expected content and a valid merged `.mcp.json`.