---
id: 35cd
title: Add color output to CLI
status: open
priority: P2
created: 2026-03-15T20:47:44.322485921Z
updated: 2026-03-15T20:47:44.322485921Z
tags:
- cli
- ux
---

Use the 'owo-colors' or 'colored' crate to colorize CLI output. P0=red, P1=yellow, P2=default, P3=dim. Status: open=blue, in_progress=cyan, done=green, blocked=red, cancelled=dim. Respect NO_COLOR env var and disable when stdout is not a tty.