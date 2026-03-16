---
id: ecb6
title: 'Epic: add task_type field to data model'
status: open
priority: P2
created: 2026-03-16T10:16:56.411653Z
updated: 2026-03-16T10:16:56.411653Z
tags:
- feature
parent: f2c5
---

Add an optional `task_type` field to the Task struct and frontmatter (values: `task` or `epic`, default `task`). Use `#[serde(default)]` so existing task files without the field continue to parse as `task`. Update `parse_task` and `render_task` accordingly. Add unit tests for parsing with and without the field.