---
id: kq8
title: Remove or replace the legacy-store scan on every load
status: open
priority: P3
created: "2026-11-20T18:25:07.046761318Z"
updated: "2026-11-20T18:25:07.046761318Z"
---

Since 0.10.0 every load checks both `.bears/` and `.bears/archive/` for a legacy `parent:` key and refuses the store (`Error::LegacyStore`). The directory that is not being loaded is read file by file and line-scanned for the word `parent` (`may_have_parent_key` in `src/store.rs`), then confirmed by the parser. That costs one file read per archived task on every command, and it grows with the archive.

It is kept in 0.10.0 because it also catches a legacy file that arrives through a git merge from an old branch.

Decide, a few releases after 0.10.0 (suggested: 0.12):

- remove the scan and leave detection to `bea doctor` (which already reports `legacy-store`), or
- replace it with a store version marker in `.bears.yml` written by `bea init` and `bea migrate`, so a load checks one value. A store without the marker would still need a scan, and a merge can bring legacy files into a marked store.

## Acceptance criteria

- A load of a migrated store no longer reads archived files it does not return, or the decision to keep the scan is recorded here with the measured cost.
- A legacy file is still found by `bea doctor`.
- `CLAUDE.md` (Legacy stores section) matches the result.