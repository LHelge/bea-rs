---
id: u6v
title: bea migrate --dry-run should preview the IDs the real run will assign
status: open
priority: P3
created: "2026-11-20T14:54:06.495537888Z"
updated: "2026-11-20T14:54:06.495537888Z"
tags:
  - ids
  - cli
---

Found in epic 8ph (ftv). `bea migrate --dry-run` draws fresh random child IDs, so the mapping it prints differs from the one the real run produces (seen: `ep1.n3` in the preview, `ep1.f6` for real). The README documents this. Options: have the dry run write the journal (`.bears/.migrate.json`) so the real run reuses it, or seed the generator deterministically from the old ID.