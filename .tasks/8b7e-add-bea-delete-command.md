---
id: 8b7e
title: Add bea delete command
status: open
priority: P1
created: 2026-03-15T20:47:44.199213646Z
updated: 2026-03-15T20:47:44.199213646Z
tags:
- cli
---

Add 'bea delete <id>' command. Prompt for confirmation unless --force flag is passed. Error clearly if task has dependents that are not done (suggest removing dep first or use --force).