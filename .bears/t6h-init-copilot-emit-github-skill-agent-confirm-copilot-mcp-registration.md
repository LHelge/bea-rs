---
id: t6h
title: 'init --copilot: emit .github skill + agent (+ confirm Copilot MCP registration)'
status: done
priority: P2
created: 2026-05-30T22:25:18.688910619Z
updated: 2026-05-31T07:31:54.812307050Z
tags:
- feature
- init
- copilot
depends_on:
- z3x
parent: j6m
---

Depends on the scaffolding framework (z3x). Skill/agent content already exists in-repo.

`bea init --copilot` emits:
- `.github/skills/bears-planning/SKILL.md` + `references/cli-fallback.md` (include_str!).
- `.github/agents/planner.agent.md` (include_str!).

Confirm how Copilot registers the MCP server and emit/merge accordingly: likely `.vscode/mcp.json` for VS Code Copilot, or repo-level config for the Copilot coding agent. If there's no clean project-local file, document the manual step instead of guessing. Reuse the framework's JSON-merge helper where a json config applies.

Integration test: temp-dir scaffold writes the expected `.github/...` files; mcp registration handled or clearly reported.