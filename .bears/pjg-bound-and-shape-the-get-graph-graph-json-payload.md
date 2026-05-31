---
id: pjg
title: Bound and shape the get_graph / graph --json payload
status: done
priority: P1
created: 2026-05-30T21:57:58.228967495Z
updated: 2026-05-31T06:59:30.999686538Z
tags:
- bug
- mcp
- graph
- performance
parent: 4vq
---

Most likely cause of the reported "get_graph never returns" in the MCP client.

Measured: `graph --json` / `get_graph` compute is fast and linear (0.30s at 8000 tasks), but it emits the FULL adjacency list of every task — 863 KB at 2000 tasks, 3.5 MB at 8000. `Graph::adjacency_list` (src/graph.rs:257-265) includes every task as a key, even ones with no edges and done/cancelled tasks. A multi-MB tool result injected into the agent context is what stalls the client.

Fix options (pick/combine):
- Exclude done/cancelled tasks by default; add optional status/epic filter and a `limit` to the MCP tool + `graph --json`.
- Omit isolated nodes (no deps and no dependents) from the adjacency map.
- Consider a more compact shape (only nodes with edges; or counts + on-demand expansion).

Keep the human `bea graph` (tree) separate — its blowup is the exponential dep_tree, tracked in dbt.

Acceptance: get_graph on a large coupled project returns a bounded, useful payload (target: well under a few hundred KB for typical projects).