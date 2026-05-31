---
id: nya
title: Codebase review hardening (v-next)
type: epic
status: done
priority: P1
created: 2026-05-30T21:37:58.665387506Z
updated: 2026-05-31T07:01:59.218070972Z
tags:
- review
- tech-debt
---

Findings from a thorough codebase review (2026-05-30). Each child task is one finding, grouped as bugs, inconsistencies, code smells, and docs.

Highest-value fixes: the TUI panic, the epic auto-close inconsistency between the `update` and `set_status` paths, and the stale `.tasks/` help text. The remainder are edge cases and cleanups.

Baseline at review time: `cargo clippy --all-targets` clean, all tests green.