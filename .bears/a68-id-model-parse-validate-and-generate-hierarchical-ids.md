---
id: a68
title: "ID model: parse, validate and generate hierarchical IDs"
status: done
priority: P1
created: "2026-11-20T12:52:25.711250465Z"
updated: "2026-11-20T13:43:33.837149076Z"
tags:
  - ids
parent: "8ph"
attempts: 1
---

In task.rs: a parsed ID type or helpers for segments, `parent_of(id)`, `is_descendant`, depth. `generate_child_id(parent, siblings, cfg)` picks a random 2-char segment, growing by one char when siblings exceed 25% of the space. Add `child-id-length` to config.rs with validation. Collision pool includes active and archived IDs. Unit tests for parsing, growth threshold, and collision avoidance.