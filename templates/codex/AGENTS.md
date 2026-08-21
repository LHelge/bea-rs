# AGENTS.md

This project uses [Bears](https://github.com/LHelge/bea-rs) for task tracking.
Bears is registered as an MCP server — use the MCP tools to manage tasks.

## Task workflow

- `list_ready` — show tasks ready to work on (all dependencies done)
- `start_task` — mark a task as in-progress before starting (`assignee` optionally claims it)
- `release_task` — hand an in-progress task back to the pool if you get stuck (status → open, assignee cleared)
- `review_task` — submit finished work for review instead of completing it outright
- `list_review` / `reject_task` — the reviewer's side: pick up work awaiting review, send it back for changes
- `propose_task` / `accept_task` — file work you are unsure about as a proposal instead of adding it to the backlog

Tasks carry an `attempts` count that increments each time work is started on them. A high count means
previous workers failed — try a different approach or escalate instead of repeating theirs.
- `complete_task` — mark a task done when finished
- `create_task` — create new tasks or epics
- `get_graph` — visualize the dependency graph
