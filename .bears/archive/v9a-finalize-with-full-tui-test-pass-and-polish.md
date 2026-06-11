---
id: v9a
title: Finalize with full TUI test pass and polish
status: done
priority: P2
created: 2026-03-17T10:14:34.709106Z
updated: 2026-03-17T12:17:05.387335Z
tags:
- tui
- test
- quality
depends_on:
- 9ad
- 88c
- d3e
- yrt
- uub
- zf9
- fd7
- qpr
parent: vbj
---

Final integration and quality gate for the whole epic.

## Scope
- Verify all child-task changes work together.
- Run full project validation: `cargo fmt && cargo clippy && cargo test`.
- Re-check TUI UX flows: navigate, edit, create, status modal, filter, reload.
- Confirm no regressions in existing TUI behavior.

## Inputs
- Depends on bug fixes (`9ad`, `88c`), refactors (`d3e`, `yrt`, `uub`), and low-severity cleanup (`zf9`, `fd7`, `qpr`).

## Acceptance Criteria
- All dependencies complete and verified together.
- Tooling checks pass cleanly.
- Any remaining risks are documented in this task body before close.
