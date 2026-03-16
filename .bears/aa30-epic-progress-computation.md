---
id: aa30
title: 'Epic: progress computation'
status: open
priority: P2
created: 2026-03-16T10:17:04.557447Z
updated: 2026-03-16T10:17:04.557447Z
tags:
- feature
depends_on:
- ecb6
parent: f2c5
---

Add a helper function `epic_progress(tasks, epic_id) -> (done, total)` that computes completion stats from child tasks (tasks whose `parent` == epic_id). Done/cancelled count as complete. Return counts and a percentage. Add unit tests.