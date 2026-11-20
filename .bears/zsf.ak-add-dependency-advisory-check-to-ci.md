---
id: zsf.ak
title: Add dependency advisory check to CI
status: done
priority: P2
created: "2026-11-20T12:52:25.703541666Z"
updated: "2026-11-20T13:20:39.448856638Z"
tags:
  - deps
  - ci
depends_on:
  - zsf.e4
attempts: 1
---

Add cargo-audit or cargo-deny to quality-checks.yml so unmaintained/vulnerable dependencies fail CI.