---
id: 8ph.s3
title: "bea migrate: convert a pre-0.10 store"
status: done
priority: P1
created: "2026-11-20T12:52:25.722967896Z"
updated: "2026-11-20T14:21:02.196328614Z"
tags:
  - ids
  - cli
depends_on:
  - 8ph.py
attempts: 1
---

One-shot command that reads legacy files (flat IDs + `parent` field), assigns hierarchical IDs to children, renames files, rewrites `depends_on`, in both .bears/ and .bears/archive/. Prints the old->new mapping, supports --dry-run, and is idempotent. Loading a legacy store without migrating should give a clear error that the CLI hints toward `bea migrate` (hint lives in hint_for(), not the library).