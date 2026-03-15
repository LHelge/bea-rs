---
id: 5dcc
title: Add release workflow triggered by git tags
status: done
priority: P2
created: 2026-03-15T22:01:02.907677996Z
updated: 2026-03-15T22:19:29.063298138Z
tags:
- ci
- infra
---

Add .github/workflows/release.yml that triggers on push of v* tags. Should build release binaries for linux-x86_64, macos-x86_64, and macos-aarch64, then create a GitHub Release and upload the binaries as assets.