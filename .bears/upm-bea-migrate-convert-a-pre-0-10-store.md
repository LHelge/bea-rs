---
id: upm
title: "bea migrate: convert a pre-0.10 store"
status: open
priority: P1
created: "2026-11-20T12:52:25.722967896Z"
updated: "2026-11-20T12:52:25.722967896Z"
tags:
  - ids
  - cli
depends_on:
  - rht
parent: "8ph"
---

One-shot command that reads legacy files (flat IDs + `parent` field), assigns hierarchical IDs to children, renames files, rewrites `depends_on`, in both .bears/ and .bears/archive/. Prints the old->new mapping, supports --dry-run, and is idempotent. Loading a legacy store without migrating should give a clear error that the CLI hints toward `bea migrate` (hint lives in hint_for(), not the library).