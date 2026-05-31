---
id: m4k
title: Make duplicate-ID handling deterministic in load_all
status: done
priority: P3
created: 2026-05-30T21:39:21.705442104Z
updated: 2026-05-30T23:04:53.541212129Z
tags:
- bug
- edge-case
- store
parent: nya
---

`load_all` (`src/store.rs:52-68`) keeps the first task seen for a duplicate id, but `JoinSet::join_next` completes in arbitrary order, so which duplicate "wins" is non-deterministic — while `find_task_path`/`save`/`delete` pick the first `read_dir` entry. The in-memory winner and on-disk winner can disagree.

Only reachable via manual file tampering, but worth making deterministic (e.g., sort paths or pick by a stable rule) and keeping the existing duplicate-ID warning.