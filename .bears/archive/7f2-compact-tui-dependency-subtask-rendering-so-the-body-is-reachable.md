---
id: 7f2
title: Compact TUI dependency/subtask rendering so the body is reachable
status: done
priority: P2
created: 2026-05-30T21:47:22.535878699Z
updated: 2026-05-31T07:31:54.825183558Z
tags:
- tui
- ux
- graph
depends_on:
- dbt
parent: 4vq
---

On larger projects the TUI detail pane (`Dependencies` + `Subtasks` sections, `src/tui/widgets/dep_tree.rs`, assembled in `src/tui/widgets/task_detail.rs`) expands every dependency under every branch, so you scroll for ages before reaching the task body.

Once dbt makes `dep_tree` expand each node once (with reference markers), wire the TUI renderer to that and make the sections compact:
- Render repeated nodes as one-line references instead of re-expanding.
- Consider a depth cap and/or collapsing the dep tree by default with a key to expand, so the body is visible without scrolling on big graphs.
- Re-check the scroll/`content_height` metrics in `task_detail.rs` against the smaller tree.

Acceptance: opening a task in a heavily-coupled project shows the body within a screen or two, not after thousands of lines.