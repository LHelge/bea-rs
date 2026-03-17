---
id: pge
title: 'Right panel: dependency branch visualization'
status: done
priority: P2
created: 2026-03-16T21:49:49.058302523Z
updated: 2026-03-17T09:37:57.484118Z
tags:
- feature
- tui
depends_on:
- 98y
parent: und
---

Add a dependency branch visualization to the right panel detail view.

- Show the upstream dependency chain for the selected task
- Use a tree or indented list format
- Mark done/open/in_progress status on each node
- Reuse graph.rs logic for building the tree