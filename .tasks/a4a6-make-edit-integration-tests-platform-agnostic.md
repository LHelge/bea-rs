---
id: a4a6
title: Make edit integration tests platform-agnostic
status: open
priority: P1
created: 2026-03-16T10:37:28.231196Z
updated: 2026-03-16T10:38:00.098088Z
tags:
- code-review
- testing
- cli
depends_on:
- 612b
---

## Context

Current edit tests rely on `sed -i` one-liners that are not portable across platforms/sed variants, causing failures on macOS.

## Scope

- Replace shell-specific `sed`-based editor simulation with a portable test helper.
- Use deterministic editor stubs/scripts created in test temp dirs.
- Keep assertions focused on task file content/resulting command behavior.

## Acceptance Criteria

- Edit integration tests pass on macOS and Linux.
- Tests do not depend on BSD/GNU `sed` syntax differences.
- Test setup clearly documents how editor behavior is simulated.

## Implementation Checklist

- [ ] Create portable editor stub helper for tests.
- [ ] Replace sed-based test invocations.
- [ ] Validate behavior on both macOS and Linux CI/local.
- [ ] Document test helper behavior in test comments.
