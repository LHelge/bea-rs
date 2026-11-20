---
id: 8ph.fc
title: Derive parent from ID and drop the parent field
status: done
priority: P1
created: "2026-11-20T12:52:25.715080934Z"
updated: "2026-11-20T13:53:59.929981834Z"
tags:
  - ids
depends_on:
  - 8ph.y2
attempts: 1
---

create_task with a parent generates a child ID under it. `Task::parent()` becomes derived; remove `parent` from frontmatter serialisation. Update epic progress, auto-close cascade, list filters (`epic`), archive cascade and graph code that read `t.parent`. Parent must still exist and be an epic.