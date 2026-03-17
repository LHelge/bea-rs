---
id: ssj
title: File watcher for live TUI updates
type: epic
status: open
priority: P1
created: 2026-03-17T21:38:57.998957451Z
updated: 2026-03-17T21:38:57.998957451Z
tags:
- tui
---

Watch the `.bears/` directory for file changes and automatically reload
tasks in the TUI so that external edits (CLI, MCP, manual) are reflected
without restarting.

## Approach
- Use the `notify` crate for cross-platform filesystem events.
- Debounce events to avoid excessive reloads on rapid writes.
- Reload tasks via existing `store::load_all` + rebuild graph.
- Preserve current selection/scroll position across reloads.

## Acceptance criteria
- External `bea create` or MCP `create_task` shows up in TUI within ~1s.
- External `bea complete` updates status in TUI detail and list views.
- Deleting a `.bears/*.md` file removes it from the TUI list.
- No flicker or lost selection on reload.