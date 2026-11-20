# Agent Workflow Guide

This document explains how to use **bears** as a task tracker with AI coding agents via the MCP (Model Context Protocol) server.

## Registering the MCP server

Bears exposes its task management tools over stdio using `bea mcp`. Register it with your AI tool:

### VS Code (Copilot / Claude)

Add to `.vscode/mcp.json`:

```json
{
  "servers": {
    "bears": {
      "command": "bea",
      "args": ["mcp"]
    }
  }
}
```

### Claude Code

Add to `.mcp.json` at your project root:

```json
{
  "mcpServers": {
    "bears": {
      "command": "bea",
      "args": ["mcp"]
    }
  }
}
```

### Cursor

Add to `.cursor/mcp.json`:

```json
{
  "mcpServers": {
    "bears": {
      "command": "bea",
      "args": ["mcp"]
    }
  }
}
```

The server must be launched from the project root (where `.bears/` lives).

## Available MCP tools

| Tool | Description |
|------|-------------|
| `list_ready` | Tasks that are open with all dependencies done (`limit?`, `tag?`, `epic?`) |
| `list_all_tasks` | List tasks with optional status/priority/tag/epic filters (`active_only?`, `include_proposed?`) |
| `list_epics` | List all epics with progress (done/total children) |
| `get_task` | Full details of a single task (falls back to the archive, marked `archived`) |
| `create_task` | Create a new task or epic; with `parent`, the new ID is created under the epic's |
| `update_task` | Update task fields (title, status, priority, tags, assignee, body); a new `parent` (`""` for none) moves the task and its subtree to new IDs |
| `start_task` | Set status to in_progress, optionally claiming it for an assignee |
| `release_task` | Release an in-progress task back to the pool (status → open, assignee cleared) |
| `list_review` | Tasks awaiting review — review work, not new work |
| `review_task` | Submit in-progress work for review instead of completing it |
| `reject_task` | Send a task under review back for changes (→ open, assignee cleared) |
| `propose_task` | Demote an open task to a proposal awaiting acceptance |
| `accept_task` | Accept a proposal into the backlog (→ open) |
| `complete_task` | Set status to done |
| `cancel_task` | Set status to cancelled |
| `prune_tasks` | Delete cancelled tasks (optionally also done tasks); returns `{ pruned, renamed }` |
| `add_dependency` | Add a dependency (cycle-safe) |
| `remove_dependency` | Remove a dependency |
| `delete_task` | Permanently delete a task; deleting an epic gives its children new top-level IDs |
| `search_tasks` | Search by text across titles, bodies, tags, and IDs |
| `plan_epic` | An epic's children in dependency order |
| `get_graph` | Bounded dependency graph as an adjacency list (`include_done?`, `epic?`, `limit?`) |
| `archive_task` | Archive a finished task (omit `id` to archive everything archivable) |
| `restore_task` | Bring an archived task back, with its archived dependencies and epic |
| `list_archived` | Archived tasks, most recent first |
| `doctor` | Read-only store check: duplicate IDs, misnamed files, dangling dependencies, cycles |

## Task IDs

IDs are hierarchical: a task created under an epic gets the epic's ID plus a segment (epic `a1b2`,
child `a1b2.c3`, grandchild `a1b2.c3.d4`), so the ID itself names the parent. There is no `parent`
field in the task files. Never edit a task's `id`, add a `parent:` key or rename a `.bears/` file by
hand: move a task with `update_task(id, parent=...)` instead.

Moving a task (`update_task` with a new `parent`), deleting an epic or pruning one renames tasks: the
result carries a `renamed` map of old ID → new ID. Use the new IDs from then on and do not reuse an
ID you noted before.

A store written before bears 0.10 still has `parent:` keys; bears refuses to load it until it is
converted once with `bea migrate` in a shell (`bea migrate --dry-run` previews the renames). There is
no MCP tool for this.

## The agentic loop

The core pattern for AI agent workflows:

```
1. list_ready              → pick the highest-priority task
2. start_task(id, assignee) → mark it in_progress and claim it
3. get_task(id)            → read the full description
4. ... do the work ...     → implement, test, commit
5. review_task(id)         → hand it to a reviewer (or complete_task to finish outright)
6. list_ready              → repeat
```

If a task turns out to be blocked or a worker gives up on it, call `release_task(id)` instead of
`complete_task` — the status goes back to `open`, the assignee is cleared, and the task reappears
in `list_ready` for whoever picks it up next.

Reviewers work the other queue: `list_review` → read the task → `complete_task` to approve, or
`reject_task` to send it back for changes. Tasks in review never show up in `list_ready`, and they
do not unblock their dependents until they are actually done. File work you are unsure about with
`propose_task` rather than adding it to the backlog outright.

Every start counts: task responses carry an `attempts` field once work has begun on them. Check it
when you pick a task up — a high count means earlier workers already failed at this, so try a
different approach or escalate rather than repeating theirs.

This loop ensures the agent always works on the most impactful unblocked task. Dependencies are respected automatically — a task only appears in `list_ready` when all its dependencies are done.

## Common workflow patterns

### Plan a feature

Break a large feature into smaller tasks with dependencies. Use epics to group related work:

```
1. create_task("Backend API", type="epic", priority="P1")
   → returns id "a1b2"

2. create_task("Design API schema", priority="P1", tags=["backend"], parent="a1b2")
   → returns id "a1b2.c3"

3. create_task("Implement endpoints", priority="P1", depends_on=["a1b2.c3"], parent="a1b2")
   → returns id "a1b2.e5"

4. create_task("Add integration tests", priority="P1", depends_on=["a1b2.e5"], parent="a1b2")
   → returns id "a1b2.g7"
```

Now `list_ready` only shows "Design API schema" until it's completed. Epics never appear in `list_ready` — they track progress automatically. When all children are done, the epic auto-closes.

Use `list_epics()` to see progress across all epics, or `list_ready(epic="a1b2")` to focus on tasks within a specific epic (the filter takes an ID or a
unique prefix and covers every task below the epic, nested epics included).

To move a task to another epic later, call `update_task(id, parent="<epic-id>")`: it and its subtree
get new IDs, every `depends_on` pointing at them is rewritten, and the result's `renamed` map tells
you the new IDs.

### Triage the backlog

Review and prioritize open tasks:

```
1. list_all_tasks(status="open")   → see all open work
2. update_task(id, priority="P0")  → escalate critical items
3. update_task(id, priority="P3")  → deprioritize low-value items
4. cancel_task(id)                 → remove tasks that are no longer relevant
```

### Explore dependencies

Understand what's blocking progress:

```
1. get_graph()                     → full dependency map
2. get_task(id)                    → check a specific task's depends_on list
3. list_all_tasks(status="blocked") → find all blocked tasks
```

## Tips

- **Always start with `list_ready`** — it respects priorities and dependencies so you work on the right thing.
- **Use tags** to scope work: `list_ready(tag="backend")` focuses on one area.
- **Use `limit`** on `list_ready` to avoid overwhelming context: `list_ready(limit=3)`.
- **Use epics** to plan larger features: create an epic, add child tasks with `parent` (their IDs extend the epic's), and track progress with `list_epics`.
- **Follow renames** — after a move, delete or prune, use the IDs in the `renamed` map.
- **Check the store** with `doctor` (or `bea doctor`) after a merge or hand edits.
- **Keep tasks small** — a task should be completable in a single focused session.
- **Add dependencies** to encode ordering constraints, not just for tracking.
