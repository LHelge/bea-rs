---
id: a259
title: Support prefix-based ID matching
status: open
priority: P1
created: 2026-03-16T09:53:16.733904Z
updated: 2026-03-16T09:53:16.733904Z
tags:
- feature
- ux
---

Allow users to specify task IDs by unique prefix instead of the full ID, like Docker container IDs. For example, `bea show 7a` should resolve to task 7ae8 if no other task ID starts with '7a'. If the prefix is ambiguous (matches multiple tasks), show an error listing the matches.