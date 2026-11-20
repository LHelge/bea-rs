---
id: "8ur"
title: Get bincode 1.3.3 out of the lockfile (RUSTSEC-2025-0141, unmaintained)
status: open
priority: P3
created: "2026-11-20T13:22:09.661487727Z"
updated: "2026-11-20T13:22:09.661487727Z"
tags:
  - deps
---

Found during the dependency refresh epic (zsf). bincode 1.3.3 comes in only through tui-markdown -> syntect and is ignored by id in `.cargo/audit.toml`. Check whether tui-markdown can be used without syntect's bincode-backed dump loading (feature flags), or whether an upstream release drops it. Remove the ignore once it is gone.