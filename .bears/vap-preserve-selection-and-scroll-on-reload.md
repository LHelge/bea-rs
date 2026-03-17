---
id: vap
title: Preserve selection and scroll on reload
status: open
priority: P2
created: 2026-03-17T21:39:17.099003133Z
updated: 2026-03-17T21:39:17.099003133Z
tags:
- tui
depends_on:
- b22
parent: ssj
---

After reload, the previously selected task should remain selected
(by id). If the selected task was deleted externally, fall back to
the nearest neighbour or the first task. Scroll offset in the detail
pane should be clamped to the new content height.

Also verify the list mode (Open/Ready/Epics/Archive/All) and any
active search query are preserved correctly across reloads.