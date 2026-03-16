---
id: 5hd
title: Wire up bea tui CLI command
status: open
priority: P1
created: 2026-03-16T21:49:49.076650196Z
updated: 2026-03-16T21:49:58.180945702Z
tags:
- feature
- tui
depends_on:
- 7k7
parent: und
---

Add the `bea tui` CLI subcommand that launches the TUI.

- Add `Tui` variant to the CLI command enum in cli.rs
- Call the TUI entry point from the handler
- Ensure it works with or without a `.bears/` directory (show init prompt if missing)