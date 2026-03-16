---
id: adw
title: Exclude epics from ready results
status: done
priority: P1
created: 2026-03-16T21:40:22.215682106Z
updated: 2026-03-16T22:17:41.227747395Z
tags:
- feature
- epic
depends_on:
- 5px
parent: hza
---

Filter epics out of `ready` results in the graph layer.

Tasks with `type: epic` should never appear in `ready()` output — they represent high-level objectives, not directly workable items.