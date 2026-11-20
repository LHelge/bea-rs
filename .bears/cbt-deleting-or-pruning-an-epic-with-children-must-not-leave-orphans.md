---
id: cbt
title: Deleting or pruning an epic with children must not leave orphans
status: done
priority: P1
created: "2026-11-20T13:59:52.599711082Z"
updated: "2026-11-20T14:34:54.959894387Z"
tags:
  - ids
depends_on:
  - rht
parent: "8ph"
attempts: 1
---

Before hierarchical IDs, delete/prune of an epic cleared its children's `parent`. Since s4t the children keep a dotted ID under a parent that no longer exists (the ID stays reserved, so nothing can adopt them, but they are orphans). Restore the old semantics with the rename machinery from rht: when an epic is deleted or pruned, detach its remaining direct children to fresh root IDs (subtrees move with them, `depends_on` rewritten in active and archived tasks) and return/print the old->new mapping. Covers `delete_task`, `prune_tasks`, CLI and MCP. Replace the interim assertion in `test_delete_scrubs_dangling_references`. Archived children of a deleted epic: decide and document (doctor, hcd, reports whatever is left).