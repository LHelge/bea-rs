---
id: "3a8"
title: Migrate this repo's .bears and release 0.10.0
status: open
priority: P2
created: "2026-11-20T12:52:25.735575644Z"
updated: "2026-11-20T14:54:06.530486587Z"
tags:
  - ids
  - release
depends_on:
  - upm
  - hcd
  - ftv
  - "6an"
  - cbt
parent: "8ph"
---

Run `bea migrate` on this repository's own store, check `bea doctor` is clean, write breaking-change release notes with the upgrade step, bump to 0.10.0.


Added by the 8ph run:
- After migrating, refresh this repo's own scaffolded agent files, which are still pre-0.10: `bea agent all --claude --copilot --codex --force` (check `CLAUDE.md` is not overwritten: use `skills` for claude if needed), covering `.claude/skills/bears-planning/*`, `.claude/agents/planner.md`, `.github/skills/*`, `.github/agents/planner.agent.md`, `.github/copilot-instructions.md`, `AGENTS.md`.
- README library snippet still says `version = "0.8"`; update with the bump.
- Install the 0.10 binary before migrating: the old `bea` and the registered MCP server cannot read child links from a migrated store.