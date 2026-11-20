---
id: 8ph.ye
title: "bea doctor: detect and repair store inconsistencies"
status: done
priority: P2
created: "2026-11-20T12:52:25.727068371Z"
updated: "2026-11-20T14:34:27.329509183Z"
tags:
  - ids
  - cli
depends_on:
  - 8ph.fc
attempts: 1
---

Checks: duplicate IDs (possible after merging two git branches), dangling depends_on, filename/ID mismatch, child whose parent is missing, non-epic parents, cycles. `--fix` renumbers duplicates (rewriting references) and repairs filenames. Library function in service.rs; CLI command plus an MCP tool returning the findings. Non-zero exit when problems are found so it can run in CI.