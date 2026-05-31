---
id: fkk
title: Trigger epic auto-close on the update_task status path too
status: done
priority: P1
created: 2026-05-30T21:38:16.353549377Z
updated: 2026-05-31T06:39:07.569188787Z
tags:
- bug
- epic
- consistency
parent: nya
---

Epic auto-close lives only in `service::set_status` (`src/service.rs:149-164`). Two paths set status to done:

- `bea done` / MCP `complete_task` → `set_status` → auto-close ✓
- `bea update --status done` / MCP `update_task {status:"done"}` → `service::update_task` (`src/service.rs:98-134`) → no auto-close ✗

Completing the last child of an epic via `update` silently leaves the epic open. Fix by funnelling both paths through one shared place that applies the status-change side effect. Add a test covering the `update --status done` → epic auto-close case.