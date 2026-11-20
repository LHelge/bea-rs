---
id: dx4
title: Decide how an epic's plan shows nested epics
status: done
priority: P3
created: "2026-11-20T18:42:27.916469312Z"
updated: "2026-11-20T22:33:17.541824955Z"
tags:
  - cli
  - mcp
attempts: 1
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
## Decision

Option 1: a plan orders the epic's direct children only, and a nested epic is one step of its parent's plan.

Reason: it is what progress, auto-close and the archive cascade already do (a nested epic is one unit of work for its parent), it needs no design for dependencies that cross sub-epics, and it is the easiest reading to change later. The `epic` filters stay as they are (whole subtree): a filter answers "what is under this epic", a plan answers "what are this epic's steps".

The nested epic's step is made useful rather than silent: in `bea show --plan` and `plan_epic` it is marked as an epic (`type: "epic"`) with its progress (`progress: { done, total }`, from `service::epic_progress`), and the Markdown output names `bea show <id> --plan` for its own steps. A child that depends on the nested epic is ordered after it; a dependency on a task inside a nested epic lies outside the plan, like one on a task outside the epic, and does not order it.
