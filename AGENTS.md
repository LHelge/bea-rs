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
| `list_ready` | Tasks that are open with all dependencies done |
| `list_all_tasks` | List tasks with optional status/priority/tag/epic filters |
| `list_epics` | List all epics with progress (done/total children) |
| `get_task` | Full details of a single task |
| `create_task` | Create a new task or epic |
| `update_task` | Update task fields (status, priority, tags, assignee, body) |
| `start_task` | Set status to in_progress |
| `complete_task` | Set status to done |
| `cancel_task` | Set status to cancelled |
| `prune_tasks` | Delete cancelled tasks (optionally also done tasks) |
| `add_dependency` | Add a dependency (cycle-safe) |
| `remove_dependency` | Remove a dependency |
| `delete_task` | Permanently delete a task |
| `search_tasks` | Search by text across titles, bodies, tags, and IDs |
| `get_graph` | Full dependency graph as an adjacency list |

## The agentic loop

The core pattern for AI agent workflows:

```
1. list_ready          → pick the highest-priority task
2. start_task(id)      → mark it in_progress
3. get_task(id)        → read the full description
4. ... do the work ... → implement, test, commit
5. complete_task(id)   → mark it done
6. list_ready          → repeat
```

This loop ensures the agent always works on the most impactful unblocked task. Dependencies are respected automatically — a task only appears in `list_ready` when all its dependencies are done.

## Common workflow patterns

### Plan a feature

Break a large feature into smaller tasks with dependencies. Use epics to group related work:

```
1. create_task("Backend API", type="epic", priority="P1")
   → returns id "a1b2"

2. create_task("Design API schema", priority="P1", tags=["backend"], parent="a1b2")
   → returns id "c3d4"

3. create_task("Implement endpoints", priority="P1", depends_on=["c3d4"], parent="a1b2")
   → returns id "e5f6"

4. create_task("Add integration tests", priority="P1", depends_on=["e5f6"], parent="a1b2")
   → returns id "g7h8"
```

Now `list_ready` only shows "Design API schema" until it's completed. Epics never appear in `list_ready` — they track progress automatically. When all children are done, the epic auto-closes.

Use `list_epics()` to see progress across all epics, or `list_ready(epic="a1b2")` to focus on tasks within a specific epic.

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
- **Use epics** to plan larger features: create an epic, add child tasks with `parent`, and track progress with `list_epics`.
- **Keep tasks small** — a task should be completable in a single focused session.
- **Add dependencies** to encode ordering constraints, not just for tracking.
