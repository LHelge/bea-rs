# CLAUDE.md

This project uses [Bears](https://github.com/LHelge/bea-rs) for task tracking.
Bears is registered as an MCP server — use the MCP tools to manage tasks.

## Task workflow

- `list_ready` — show tasks ready to work on (all dependencies done)
- `start_task` — mark a task as in-progress before starting (`assignee` optionally claims it)
- `release_task` — hand an in-progress task back to the pool if you get stuck (status → open, assignee cleared)
- `list_stale` — find in-progress tasks nobody has updated for `hours` hours (default 24), claims a dead worker may have left; it only lists, `release_task` hands one back
- `review_task` — submit finished work for review instead of completing it outright
- `list_review` / `reject_task` — the reviewer's side: pick up work awaiting review, send it back for changes
- `propose_task` / `accept_task` — file work you are unsure about as a proposal instead of adding it to the backlog
- `complete_task` — mark a task done when finished
- `create_task` — create new tasks or epics
- `get_graph` — visualize the dependency graph

`bea prime` prints the ready, in-progress and review queues and a reminder of the workflow verbs, for
a session-start hook. The reminder gives the verbs in their CLI form (`bea start`, `bea review`, …);
while the Bears MCP server is connected, use the MCP tools of the same names listed above. To have it in every session, add to `.claude/settings.json`:

```json
{ "hooks": { "SessionStart": [{ "hooks": [{ "type": "command", "command": "test ! -d .bears || bea prime" }] }] } }
```

Tasks carry an `attempts` count that increments each time work is started on them. A high count means
previous workers failed — try a different approach or escalate instead of repeating theirs.

Task IDs are hierarchical: a task under an epic has the epic's ID as its prefix (`a1b` → `a1b.c2`). If bears refuses to load the store because it predates hierarchical IDs, run `bea migrate` once (`bea migrate --dry-run` previews it).
Moving a task (`update_task` with `parent`) or deleting an epic renames tasks; the result carries a
`renamed` map of old ID → new ID. Use the new IDs and do not reuse ones you noted earlier.
