---
id: 8b7e
title: Add bea delete command
status: in_progress
priority: P1
created: 2026-03-15T20:47:44.199213646Z
updated: 2026-03-15T22:32:43.372473165Z
tags:
- cli
---

Add 'bea delete <id>' command. Prompt for confirmation unless --force flag is passed. Error clearly if task has dependents that are not done (suggest removing dep first or use --force).