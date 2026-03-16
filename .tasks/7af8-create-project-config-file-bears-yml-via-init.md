---
id: 7af8
title: Create project config file .bears.yml via init
status: open
priority: P1
created: 2026-03-16T10:46:46.739127Z
updated: 2026-03-16T10:46:46.739127Z
tags:
- code-review
- config
- init
---

## Context

We need project-level configuration for behavior that should be configurable per repo. The first concrete setting is task ID length, which is currently hard-coded.

## Scope

- Add support for a project config file at project root: `.bears.yml`.
- Generate this file during `bea init`.
- Initial config content should include `id-length: 4`.
- Add config loading and validation with sensible fallback behavior.

## Acceptance Criteria

- `bea init` creates `.bears.yml` in project root.
- Generated config includes `id-length: 4` by default.
- CLI and MCP can read config without breaking existing repos.
- Invalid config values produce clear errors.

## Implementation Checklist

- [ ] Define config struct and YAML schema.
- [ ] Implement load/read helpers with validation.
- [ ] Update init command to create `.bears.yml`.
- [ ] Add tests for init output and config parsing.