# AGENTS.md

This project uses [Bears](https://github.com/LHelge/bea-rs) for task tracking.
Bears is registered as an MCP server — use the MCP tools to manage tasks.

## Task workflow

- `list_ready` — show tasks ready to work on (all dependencies done)
- `start_task` — mark a task as in-progress before starting
- `complete_task` — mark a task done when finished
- `create_task` — create new tasks or epics
- `get_graph` — visualize the dependency graph
