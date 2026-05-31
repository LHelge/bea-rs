---
id: 5kx
title: Refresh CLAUDE.md architecture section to match current code
status: done
priority: P2
created: 2026-05-30T21:39:31.392900874Z
updated: 2026-05-31T07:01:59.217839152Z
tags:
- docs
parent: nya
---

CLAUDE.md's architecture section predates a lot of the code. It doesn't mention the `tui/` module, `editor.rs`, the `edit`/`tui` commands, `effective_priority`, or `show --plan`. Since this file steers future work (and we're about to plan v-next features), bring the module map, command list, and design-principles sections up to date.