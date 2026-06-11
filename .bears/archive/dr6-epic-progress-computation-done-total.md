---
id: dr6
title: Epic progress computation (done/total)
status: done
priority: P1
created: 2026-03-16T21:40:22.209900341Z
updated: 2026-03-16T22:21:08.129421684Z
tags:
- feature
- epic
depends_on:
- 5px
parent: hza
---

Add epic progress computation to the service/graph layer.

- Given an epic ID, count children by status (done / total)
- Return as a struct: `EpicProgress { done: usize, total: usize }`
- Include in epic listing output