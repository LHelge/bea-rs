---
id: h4j.8v
title: Document the archive feature in CLAUDE.md
status: done
priority: P3
created: "2026-05-30T22:13:56.140519751Z"
updated: "2026-05-31T08:01:01.202766940Z"
tags:
  - feature
  - archive
  - docs
depends_on:
  - h4j.hm
  - h4j.k7
---

Depends on the CLI (awg) and MCP (5wj) tasks. Document the archive feature in CLAUDE.md:
- `.bears/archive/` storage location and that load_all skips it.
- The archivability invariant (terminal + no active dependents) and cascade direction.
- `archive` / `restore` / `list --archived` CLI commands.
- The new MCP tools and that they're hidden from default listings.
- That `prune` remains hard-delete (archive is the soft option).

Coordinate with 5kx (the broader CLAUDE.md refresh) to avoid conflicting edits.