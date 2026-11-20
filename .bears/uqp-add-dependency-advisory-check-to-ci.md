---
id: uqp
title: Add dependency advisory check to CI
status: open
priority: P2
created: "2026-11-20T12:52:25.703541666Z"
updated: "2026-11-20T12:52:25.703541666Z"
tags:
  - deps
  - ci
depends_on:
  - "9up"
parent: zsf
---

Add cargo-audit or cargo-deny to quality-checks.yml so unmaintained/vulnerable dependencies fail CI.