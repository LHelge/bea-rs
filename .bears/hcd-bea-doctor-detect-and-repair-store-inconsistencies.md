---
id: hcd
title: "bea doctor: detect and repair store inconsistencies"
status: open
priority: P2
created: "2026-11-20T12:52:25.727068371Z"
updated: "2026-11-20T12:52:25.727068371Z"
tags:
  - ids
  - cli
depends_on:
  - s4t
parent: "8ph"
---

Checks: duplicate IDs (possible after merging two git branches), dangling depends_on, filename/ID mismatch, child whose parent is missing, non-epic parents, cycles. `--fix` renumbers duplicates (rewriting references) and repairs filenames. Library function in service.rs; CLI command plus an MCP tool returning the findings. Non-zero exit when problems are found so it can run in CI.