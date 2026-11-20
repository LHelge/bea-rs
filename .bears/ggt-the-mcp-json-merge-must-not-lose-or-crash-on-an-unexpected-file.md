---
id: ggt
title: The .mcp.json merge must not lose or crash on an unexpected file
status: open
priority: P2
created: "2026-11-20T22:58:19.509455238Z"
updated: "2026-11-20T22:58:19.509455238Z"
tags:
  - bug
  - scaffold
---

Found doing vfb. `scaffold::merge_mcp_json` (run by `bea init` / `bea agent skills|all` with `--claude` or `--copilot`) handles a bad file badly: an existing `.mcp.json` that is not valid JSON is silently replaced by `{}` plus the bears entry, so the user's other servers are lost; and a top level that is not an object, or a `mcpServers` / `servers` that is not an object, makes it panic on `expect`. Behave like `merge_session_hook` (added by vfb): leave the file byte for byte as it was, print a one-line warning naming the path and the reason, and do not list it as written. Tests for each case, for both harnesses.