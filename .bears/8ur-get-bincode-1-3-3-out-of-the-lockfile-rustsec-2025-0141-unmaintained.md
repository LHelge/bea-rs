---
id: 8ur
title: Get bincode 1.3.3 out of the lockfile (RUSTSEC-2025-0141, unmaintained)
status: open
priority: P3
created: "2026-11-20T13:22:09.661487727Z"
updated: "2026-11-20T22:29:54.503904520Z"
tags:
  - deps
attempts: 1
---

Found during the dependency refresh epic (zsf). bincode 1.3.3 comes in only through tui-markdown -> syntect and is ignored by id in `.cargo/audit.toml`. Check whether tui-markdown can be used without syntect's bincode-backed dump loading (feature flags), or whether an upstream release drops it. Remove the ignore once it is gone.

## Finding (2026-11-20): blocked upstream

- Path: bea-rs `cli` -> tui-markdown 0.3.13 default feature `highlight-code` -> syntect 5.3.0 (`default-features = false`; features `default-syntaxes`, `default-themes`, `plist-load`, `regex-onig`). `default-syntaxes` and `default-themes` both need `dump-load` = `[flate2, bincode]`: syntect's bundled syntaxes and themes are bincode dumps.
- Upstream: tui-markdown 0.3.13 and syntect 5.3.0 are the latest releases (`cargo search` / `cargo info`); neither drops bincode 1.x. tui-markdown's only features are `highlight-code` and `document-features`.
- The only switch is `default-features = false` on tui-markdown. It removes syntect and bincode, but also the syntax colouring of recognised fenced code blocks (```rust, ```bash, ```json, ...) in the TUI's task body view (`src/tui/widgets/body.rs`, `tui_markdown::from_str`); they would render in the plain code style. That is user-visible (8 task files in this repository's own store use language fences), so the change was not made.
- Unblocks when a syntect release moves `dump-load` off bincode 1.x, or tui-markdown can highlight without syntect's bundled dumps. Then bump, drop the `RUSTSEC-2025-0141` ignore from `.cargo/audit.toml` and run `cargo audit`.