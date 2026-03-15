---
id: ea65
title: Set up GitHub Actions CI
status: done
priority: P0
created: 2026-03-15T20:46:36.139186465Z
updated: 2026-03-15T21:49:29.887653537Z
tags:
- ci
- infra
---

Add .github/workflows/ci.yml that runs cargo fmt --check, cargo clippy -- -D warnings, and cargo test on push and PR. Target: ubuntu-latest with stable toolchain.