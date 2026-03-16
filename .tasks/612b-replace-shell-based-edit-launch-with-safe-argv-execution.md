---
id: 612b
title: Replace shell-based edit launch with safe argv execution
status: open
priority: P1
created: 2026-03-16T10:37:28.223571Z
updated: 2026-03-16T10:37:28.223571Z
tags:
- code-review
- cli
- security
---

## Context

`bea edit` currently launches the editor through `sh -c`, which is brittle across platforms and increases command-injection risk in untrusted environments.

## Scope

- Replace shell-based invocation with direct process spawning.
- Parse `$EDITOR`/`$VISUAL` into argv safely (support editor args).
- Preserve fallback behavior to `vi` when env vars are unset.
- Improve error reporting for missing editor executable and non-zero exit.

## Acceptance Criteria

- `bea edit` works with common editor settings that include arguments.
- No `sh -c` invocation remains in edit path.
- Behavior is consistent on macOS and Linux.
- Integration and unit tests cover success, failure, and no-change flows.

## Implementation Checklist

- [ ] Replace shell invocation with direct process execution.
- [ ] Parse editor env var into executable and args.
- [ ] Preserve fallback and exit-code handling.
- [ ] Add tests for editor parsing and error paths.
