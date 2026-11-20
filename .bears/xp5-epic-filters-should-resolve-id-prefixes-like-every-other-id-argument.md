---
id: xp5
title: Epic filters should resolve ID prefixes like every other ID argument
status: done
priority: P3
created: "2026-11-20T14:54:06.500563889Z"
updated: "2026-11-20T18:40:16.278569554Z"
tags:
  - cli
  - mcp
attempts: 1
---

Found in epic 8ph (ftv); predates it. `bea list --epic`, `bea ready --epic`, `bea review --epic` and the MCP `epic` params match the exact ID only, with no `store::resolve_prefix`. Every other ID argument accepts a unique prefix. With dotted IDs (`a1b.c2`) typing the full epic ID is more of a burden.