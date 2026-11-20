---
id: dx4
title: Decide how an epic's plan shows nested epics
status: open
priority: P3
created: "2026-11-20T18:42:27.916469312Z"
updated: "2026-11-20T18:42:27.916469312Z"
tags:
  - cli
  - mcp
---

Found while doing `akg` (epic filters match the whole subtree), after epic `8ph`.

`bea show <epic> --plan` and MCP `plan_epic` order an epic's direct children only. The tasks of a nested epic (`a1b.c2.d7` under `a1b.c2` under `a1b`) do not appear in the plan of `a1b`; the nested epic is at most one entry. Since `akg`, `ready --epic a1b` and the other `epic` filters do reach those tasks, so the plan and the filters disagree on what is in an epic.

Options:

- Keep direct children only and say so in the output and the docs.
- Flatten: order every descendant task in one plan. Simple, loses the grouping.
- Nest: show each sub-epic as a step that expands into its own ordered tasks. Needs a design for dependencies that cross sub-epics.

## Acceptance criteria

- One option chosen and recorded here.
- `bea show --plan`, `plan_epic`, `CLAUDE.md`, `README.md` and the templates agree on what a plan contains.
- A test with a nested epic pins the behaviour.