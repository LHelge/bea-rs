---
id: s4r
title: Reword YAML library advice in frontmatter warnings
status: done
priority: P3
created: "2026-11-20T13:15:44.793234367Z"
updated: "2026-11-20T22:21:08.463479826Z"
tags:
  - ux
attempts: 1
---

Found during the dependency refresh epic (zsf). Since the move to serde-saphyr, some skipped-file warnings carry advice aimed at Rust developers, e.g. `assignee: ~` gives "cannot deserialize null into string; use Option<String> at line 7, column 11" and a duplicate key gives "... set DuplicateKeyPolicy in Options if acceptable". Strip or reword that advice so the warning states what is wrong in the file and where. Keep it one line; keep the position.