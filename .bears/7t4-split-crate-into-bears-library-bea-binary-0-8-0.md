---
id: "7t4"
title: Split crate into `bears` library + `bea` binary (0.8.0)
status: done
priority: P1
created: "2026-08-20T13:26:45.018179Z"
updated: "2026-08-20T13:32:28.362020Z"
tags:
  - refactor
  - api
---

Make the core usable as a crate dependency so agent harnesses can embed bears without shelling out to the `bea` CLI.

- Add `src/lib.rs` exposing `config`, `error`, `graph`, `scaffold`, `service`, `store`, `task`
- `[lib] name = "bears"` (package stays `bea-rs`; `bears` is taken on crates.io)
- Keep `cli`, `mcp`, `tui`, `editor` binary-private
- Feature-gate frontend deps (clap, ratatui, crossterm, rmcp, ...) behind a default `cli` feature so library consumers can use `default-features = false`
- Errors carry no frontend hints; `NotInitialized` message genericized and the `bea init` hint moved to main.rs
- Bump to 0.8.0 (breaking)