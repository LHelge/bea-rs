---
id: akg
title: Epic filters match the whole subtree, not only direct children
status: open
priority: P2
created: "2026-11-20T18:31:07.852613372Z"
updated: "2026-11-20T18:31:07.852613372Z"
---

With hierarchical IDs an epic can contain an epic (`a1b` → `a1b.c2` → `a1b.c2.d7`). The `epic` filters still match direct children only, so `bea ready --epic a1b` returns nothing for work under the nested epic `a1b.c2`: epics are never ready, and their children are not direct children of `a1b`. An agent told to work on `a1b` concludes there is nothing to do.

Decided by the user while reviewing the pull request for epic `8ph`: the filters answer "what is in this epic", so they match every descendant.

## Scope

- CLI: `bea list --epic`, `bea ready --epic`, `bea review --epic`, and `bea graph --epic` if it has the filter.
- MCP: the `epic` parameter of `list_ready`, `list_all_tasks`, `list_review`, `get_graph`.
- The TUI, if it filters by epic through the same service code.

A task matches when its ID is a strict descendant of the epic's ID (`task::is_descendant`). The epic itself is not in the result, as today.

## Not in scope

Progress in `bea epics` / `list_epics`, auto-close, and the archive cascade stay on direct children: a nested epic is one unit of work for its parent, and auto-close already cascades up.

## Acceptance criteria

- `ready --epic a1b` returns a ready task `a1b.c2.d7` under a nested epic; the same for `list`, `review` and the MCP tools.
- A nested epic itself appears in `list --epic a1b` (as a direct child does today) and never in `ready`.
- `bea epics` progress, auto-close and archiving an epic behave as before; tests pin that.
- `CLAUDE.md` ("Children counts, epic filters and auto-close use direct children only"), `README.md`, the MCP tool descriptions and the templates say what the filter matches.
- Do together with `xp5` (the same filters should accept an ID prefix); it touches the same code.