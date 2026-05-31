---
id: 2kx
title: MCP API parity & ergonomics
type: epic
status: done
priority: P2
created: 2026-05-30T22:01:03.602095099Z
updated: 2026-05-31T06:59:30.510848324Z
tags:
- mcp
- api
---

Gaps found comparing the MCP tool surface (src/mcp/tools.rs, params.rs) against the CLI (src/cli/args.rs) and service layer:

- update_task cannot set title (hardcoded None at tools.rs:146) — service already supports it.
- No reparenting end to end: service::update_task has no `parent` param, so neither MCP nor CLI `update` can move a task; only editing the .bears file works. create_task also does not validate `parent`.
- No MCP equivalent of `show --plan` (service::plan_epic is CLI-only).
- Filtering/limit asymmetry: list_all_tasks forces include_all and has no limit; search_tasks forces include_all; only list_ready has a limit.

Theme: bring the MCP surface to parity with the CLI and make it ergonomic for agents, while keeping payloads bounded (see pjg in the graph-perf epic).