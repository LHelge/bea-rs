---
id: 3z9
title: Test file watcher reload behaviour
status: open
priority: P2
created: 2026-03-17T21:39:22.418173539Z
updated: 2026-03-17T21:39:22.418173539Z
tags:
- tui
depends_on:
- vap
parent: ssj
---

Write integration tests that:
- Start the watcher on a temp `.bears/` directory.
- Create/modify/delete task files externally.
- Assert that the reload signal fires and `App` state updates correctly.

Unit-test the debounce logic to ensure rapid writes produce a single reload.