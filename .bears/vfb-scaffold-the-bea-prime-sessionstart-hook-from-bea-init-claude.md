---
id: vfb
title: Scaffold the bea prime SessionStart hook from bea init --claude
status: done
priority: P3
created: "2026-11-20T22:21:48.695103115Z"
updated: "2026-11-20T22:57:46.595833981Z"
tags:
  - agents
  - cli
attempts: 1
---

Follow-up to 8z2.xp, which documents the hook (README, Claude template) but does not scaffold it. `bea init --claude` / `bea agent skills --claude` could merge a `SessionStart` hook running `test ! -d .bears || bea prime` into `.claude/settings.json`, like the `.mcp.json` merge: only the bears entry touched, idempotent, existing hooks and settings preserved. Decide first whether it is opt-in (a flag) and how it behaves with the overwrite guard. Also verify that Claude Code runs the hook in the project directory; if not, the command needs `cd "$CLAUDE_PROJECT_DIR"` first.