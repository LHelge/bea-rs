---
id: vbj
title: TUI hardening and simplification
type: epic
status: done
priority: P1
created: 2026-03-17T10:14:28.391199Z
updated: 2026-03-17T12:17:05.387783Z
tags:
- tui
- quality
- refactor
---

This epic tracks the full TUI review follow-up and maps each finding to actionable work.

## Review Findings Covered
- High: editor launch failures are currently silent; create/edit can appear successful even when editor launch failed.
- High: editor command parsing is fragile (`split_whitespace`) and can break quoted args or executable paths containing spaces.
- Medium: recoverable runtime errors in TUI actions currently bubble up and can exit the loop.
- Medium: duplicated load/sort/reload logic in TUI runtime paths.
- Medium: duplicated input handling and duplicated prompt rendering for create/filter modes.
- Low: filter matching path performs avoidable repeated lowercase allocations.
- Low: spec/scope mismatch around delete operation listed in TUI notes but not currently wired as an action.
- Low: bottom bar hint overflow risk on narrow terminals.
- Low: grapheme/UTF-8 prompt editing behavior needs explicit validation.
- Testing gap: no focused tests for editor failure behavior and action-level error handling in the main event flow.

## Code Hotspots From Review
- `src/tui/mod.rs`: editor launch path, event-loop action dispatch, repeated load/sort/reload calls.
- `src/tui/app.rs`: duplicated input handlers, duplicated input prompt rendering, filter matching path.
- `.bears/und-tui-support.md`: operation list includes delete, which should align with implemented TUI scope.

## Execution Order
1. `x4j` tests first, to lock expected behavior.
2. `9ad` and `88c` implement bug fixes on top of those tests.
3. `d3e` then `yrt` for readability/dedup.
4. `uub` for filter simplification/perf.
5. `zf9`, `fd7`, `qpr` to close low-severity but important UX correctness gaps.
6. `v9a` final pass (`cargo fmt && cargo clippy && cargo test`).

## Done Criteria
- All child tasks in this epic are `done`.
- No behavior regressions in TUI edit/create/status/filter flows.
- TUI code paths are simpler to follow with less duplication.
- The documented scope for TUI behavior matches implementation.