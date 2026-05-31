---
id: nsm
title: Add limit + active-only filtering parity to list_all_tasks and search_tasks (MCP)
status: done
priority: P3
created: 2026-05-30T22:01:28.670656562Z
updated: 2026-05-31T06:59:30.510546732Z
tags:
- mcp
- api
- performance
parent: 2kx
---

MCP output/filtering differs from the CLI and can produce unbounded payloads:
- `list_all_tasks` hardcodes include_all=true (tools.rs:54) and has no `limit`. Measured ~80 KB at 2000 tasks.
- `search_tasks` hardcodes include_all=true (tools.rs:224).
- Only `list_ready` exposes a `limit`.

Add for parity:
- Optional `limit` on `list_all_tasks` (and consider `search_tasks`).
- An active-only toggle (e.g. `include_done`/`active_only`) on `list_all_tasks` and `search_tasks` so callers can exclude done/cancelled like the CLI default.

Keeps agent payloads bounded; complements the get_graph payload work (pjg).