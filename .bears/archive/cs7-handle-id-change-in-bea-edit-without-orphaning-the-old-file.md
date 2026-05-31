---
id: cs7
title: Handle id change in `bea edit` without orphaning the old file
status: done
priority: P2
created: 2026-05-30T21:38:45.178616603Z
updated: 2026-05-30T22:55:53.933048316Z
tags:
- bug
- cli
- edge-case
parent: nya
---

`cmd_edit` (`src/cli/cmd.rs:584-620`) re-parses the edited file and calls `store::save`, which locates the old file via `find_task_path(base, &t.id)`. If the user edits the `id:` frontmatter field, save looks up the *new* id, fails to find the old file, and writes a second file — orphaning the original.

Options: reject id changes on edit (compare parsed id to original and error), or track the original id and rename/remove the old file explicitly. Add a test for editing the id field.