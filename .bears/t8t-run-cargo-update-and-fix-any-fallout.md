---
id: t8t
title: Run cargo update and fix any fallout
status: done
priority: P1
created: "2026-11-20T12:52:25.696217655Z"
updated: "2026-11-20T13:01:55.207480636Z"
tags:
  - deps
parent: zsf
attempts: 1
---

Manifest versions are already at latest majors. `cargo update` bumps ~78 lockfile entries (tokio 1.54, tui-markdown 0.3.13, thiserror 2.0.21, ...). Run fmt/clippy/test and `cargo build --lib --no-default-features` afterwards.