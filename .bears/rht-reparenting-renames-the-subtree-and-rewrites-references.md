---
id: rht
title: Reparenting renames the subtree and rewrites references
status: done
priority: P1
created: "2026-11-20T12:52:25.719017164Z"
updated: "2026-11-20T14:06:50.925557398Z"
tags:
  - ids
depends_on:
  - s4t
parent: "8ph"
attempts: 1
---

`update --parent` / update_task parent: compute new IDs for the task and all descendants, rename files, and rewrite `depends_on` in every active and archived task. Detaching (`""`) produces a fresh root ID. Return the old->new ID mapping to the caller (CLI prints it, MCP returns it). Must be safe if interrupted: write new files before removing old ones. Tests: subtree move, archived dependants, detach, cycle/self-parent rejection.