---
id: taw
title: Report frontmatter error positions as lines of the file
status: open
priority: P3
created: "2026-11-20T22:21:48.670391034Z"
updated: "2026-11-20T22:21:48.670391034Z"
tags:
  - ux
---

Found doing s4r. A frontmatter parse error's position counts from the first line after the opening `---`, so it is one line short of the line in the file (`assignee: ~` on file line 8 is reported as line 7). Give `parse_task` a line offset (serde-saphyr's `RenderOptions::line_offset`) so the warning names the file's line, without changing the numbering of `.bears.yml` errors. Update the pinned messages in `task.rs` and `tests/cli.rs`.