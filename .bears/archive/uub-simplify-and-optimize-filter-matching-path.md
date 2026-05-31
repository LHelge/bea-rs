---
id: uub
title: Simplify and optimize filter matching path
status: done
priority: P2
created: 2026-03-17T10:14:34.703324Z
updated: 2026-03-17T11:30:44.825150Z
tags:
- tui
- perf
- readability
parent: vbj
---

Improve filter readability and avoid repeated lowercase work.

## Problem
Current matching lowercases query and task fields repeatedly per task evaluation, causing avoidable allocations/work and noisier logic.

## Scope
- Refactor matching flow to normalize query once per filter application.
- Simplify match checks for title/id/tags and document intended searchable fields.
- Decide whether to include additional fields (body/assignee/parent) or document current behavior explicitly.

## Acceptance Criteria
- Matching code is simpler to read.
- Repeated lowercase conversions are reduced.
- Behavior is covered by tests and remains predictable.
