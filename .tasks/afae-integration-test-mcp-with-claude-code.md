---
id: afae
title: Integration test MCP with Claude Code
status: done
priority: P1
created: 2026-03-15T20:48:21.730815951Z
updated: 2026-03-15T21:58:22.972748273Z
tags:
- mcp
- testing
depends_on:
- '7895'
---

Register the MCP server in Claude Code config and manually verify all 11 tools work correctly end-to-end. Test: list_ready, create_task, start_task, complete_task, add_dependency cycle rejection, search_tasks. Document any issues found.