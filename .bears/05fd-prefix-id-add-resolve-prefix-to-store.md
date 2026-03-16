---
id: 05fd
title: 'Prefix ID: add resolve_prefix to store'
status: open
priority: P1
created: 2026-03-16T09:53:26.678314Z
updated: 2026-03-16T10:49:57.021262Z
tags:
- feature
- ux
depends_on:
- 6d54
parent: a259
---

Add a `resolve_prefix(prefix: &str) -> Result<Task>` function to the store that iterates loaded tasks and returns the unique match, or an error if zero or multiple tasks match. Include unit tests for exact match, unique prefix, ambiguous prefix, and no match.