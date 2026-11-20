---
id: bjt.nv
title: CI workflow that publishes the snapshot
status: done
priority: P3
created: "2026-11-20T12:52:25.761611191Z"
updated: "2026-11-20T21:05:51.023396315Z"
tags:
  - snapshot
  - ci
depends_on:
  - bjt.r9
attempts: 1
---

Dogfood in this repo: write the Markdown snapshot to $GITHUB_STEP_SUMMARY and upload the HTML as a build artifact. Add a copy-pasteable workflow example to the README for users.