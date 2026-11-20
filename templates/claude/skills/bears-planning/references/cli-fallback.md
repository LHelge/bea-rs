# Bears CLI Fallback Reference

Use these commands when the Bears MCP tools (`mcp__bears__*`) are not available. Always pass `--json` for structured output.

Task IDs are hierarchical: a task created with `--parent <epic-id>` gets an ID under the epic's (`a1b` → `a1b.c2`). Commands that move or detach tasks report the new IDs in a `renamed` map of old ID → new ID; use the new IDs afterwards. If bears refuses to load the store because it predates hierarchical IDs, run `bea migrate` once (`bea migrate --dry-run` previews it).

## Task Creation

```bash
# Create a task
bea create "Task title" --priority P1 --json

# Create an epic
bea create "Epic title" --epic --priority P1 --json

# Create under an epic, with tags (the ID is created under the epic's: a1b.c2)
bea create "Sub-task" --priority P1 --parent <epic-id> --tag backend,api --json

# Create with dependencies
bea create "Next step" --priority P1 --depends-on <id1>,<id2> --json

# Create with body
bea create "Task" --priority P1 --body "Description here" --json
```

## Dependencies

```bash
# Add a dependency (task <id> depends on <dep-id>)
bea dep add <id> <dep-id> --json

# Remove a dependency
bea dep remove <id> <dep-id> --json
```

## Querying

```bash
# List all open tasks
bea list --json

# List with filters
bea list --status open --priority P1 --tag backend --json

# List tasks under an epic (every level: tasks under its nested epics too)
bea list --epic <epic-id> --json

# Show ready tasks (open, all deps done)
bea ready --json
bea ready --epic <epic-id> --limit 5 --json

# Session-start context: ready (top 10, or --limit N), in-progress and review queues
bea prime --json
bea prime --limit 3 --json

# List epics with progress
bea epics --json

# Show a single task
bea show <id> --json

# An epic's plan: its direct children in execution order. A nested epic is one
# step with "progress": { done, total }; run --plan on its ID for its own steps
bea show <epic-id> --plan --json

# Search tasks
bea search "query" --json

# Show dependency graph
bea graph --json
```

## Status Changes

```bash
# Start a task (set in_progress), optionally claiming it
bea start <id> --json
bea start <id> --assignee <name> --json

# Release an in-progress task back to the pool (open, assignee cleared)
bea release <id> --json

# List in-progress tasks not updated for 24 hours (or --hours N); lists only
bea stale --json
bea stale --hours 4 --json

# Complete a task (set done)
bea done <id> --json

# Submit finished work for review, or list the review queue
bea review <id> --json
bea review --json

# Send a task under review back for changes
bea reject <id> --json

# Propose work without adding it to the backlog, and accept a proposal
bea propose <id> --json
bea accept <id> --json

# Cancel a task
bea cancel <id> --json
```

## Updates

```bash
# Update priority
bea update <id> --priority P0 --json

# Update tags
bea update <id> --tag tag1,tag2 --json

# Update body
bea update <id> --body "New description" --json

# Update status
bea update <id> --status blocked --json

# Move a task under another epic ("" for the top level). The task and its subtree
# get new IDs; the result carries "renamed": { "<old-id>": "<new-id>", ... }
bea update <id> --parent <epic-id> --json
bea update <id> --parent "" --json
```

## Deletion

```bash
# Delete a task permanently. Deleting an epic gives its children new top-level
# IDs, listed in the result's "renamed" map
bea delete <id> --json

# Prune cancelled tasks
bea prune --json

# Prune cancelled and done tasks
bea prune --done --json

# Prune always returns { "pruned": [...], "renamed": { ... } }: read the deleted
# tasks from .pruned; "renamed" maps the old to the new ID of each surviving
# child of a pruned epic, and is empty when nothing moved
```

## Store health

```bash
# Check the store for duplicate IDs, misnamed files, dangling dependencies, cycles
bea doctor --json
```
