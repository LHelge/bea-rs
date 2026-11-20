---
id: hwd
title: Cap slug length so long titles do not exceed the filename limit
status: done
priority: P2
created: "2026-11-20T13:15:44.788039295Z"
updated: "2026-11-20T21:43:02.594838999Z"
tags:
  - bug
attempts: 1
---

Found during the dependency refresh epic (zsf), pre-existing. A title of about 300 characters fails with "File name too long (os error 36)" because `task::slugify` / `task::filename` do not cap the slug. Truncate the slug (on a char boundary, trimming a trailing dash) so `{id}-{slug}.md` stays well under 255 bytes. Existing files must still be found by ID after the change. Test with a 300-char and a multi-byte title.