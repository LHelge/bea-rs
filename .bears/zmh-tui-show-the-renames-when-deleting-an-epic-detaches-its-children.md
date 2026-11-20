---
id: zmh
title: "TUI: show the renames when deleting an epic detaches its children"
status: open
priority: P3
created: "2026-11-20T14:59:24.614049666Z"
updated: "2026-11-20T14:59:24.614049666Z"
tags:
  - tui
  - ids
---

Found in the epic review of 8ph. `src/tui/mod.rs:217-227`: deleting an epic in the TUI detaches and renames its children (fresh root IDs) without telling the user; it ignores `DeletedTask.renamed` and just reloads. CLI and MCP both report the `renamed` map. Show it in the status/bottom bar or a modal.