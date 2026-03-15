---
id: 78b3
title: Publish releases to crates.io
status: cancelled
priority: P1
created: 2026-03-15T22:17:54.833187625Z
updated: 2026-03-15T22:18:47.248644350Z
tags:
- ci
- release
depends_on:
- 4ec7
---

Add a step in the release workflow (or a separate workflow) that publishes the crate to crates.io on tagged releases. Requires CARGO_REGISTRY_TOKEN secret to be set in the repository settings.