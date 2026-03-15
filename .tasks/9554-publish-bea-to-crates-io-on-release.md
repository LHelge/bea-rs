---
id: '9554'
title: Publish bea to crates.io on release
status: open
priority: P2
created: 2026-03-15T22:01:08.235521128Z
updated: 2026-03-15T22:01:08.235521128Z
tags:
- ci
- infra
depends_on:
- 5dcc
---

Extend the release workflow (or add a separate job) to run cargo publish when a v* tag is pushed. Requires CARGO_REGISTRY_TOKEN secret to be set in the repo.