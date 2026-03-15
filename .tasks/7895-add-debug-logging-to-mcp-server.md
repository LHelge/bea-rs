---
id: '7895'
title: Add debug logging to MCP server
status: open
priority: P2
created: 2026-03-15T20:48:21.792806982Z
updated: 2026-03-15T20:48:21.792806982Z
tags:
- mcp
---

When BEA_LOG=debug env var is set, write JSON-RPC request/response pairs to stderr (or a log file). Include timestamps. This helps diagnose issues when the server is registered in an AI client. Do not log to stdout as that breaks the JSON-RPC protocol.