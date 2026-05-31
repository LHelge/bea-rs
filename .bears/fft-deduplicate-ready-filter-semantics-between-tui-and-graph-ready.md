---
id: fft
title: Deduplicate ready/filter semantics between TUI and graph::ready
status: done
priority: P3
created: 2026-05-30T21:38:40.090193371Z
updated: 2026-05-31T06:39:07.543440605Z
tags:
- refactor
- tui
parent: nya
---

`App::apply_filter` (`src/tui/app.rs:203-228`) hand-rolls the "all deps done" readiness check that already exists in `graph::ready`. Two copies of the readiness rule that can drift (e.g., missing-dep handling). Have the TUI Ready mode reuse the shared graph routine instead of reimplementing it.