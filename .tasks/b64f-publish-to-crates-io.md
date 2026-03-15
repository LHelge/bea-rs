---
id: b64f
title: Publish to crates.io
status: done
priority: P1
created: 2026-03-15T20:47:09.761816456Z
updated: 2026-03-15T22:31:27.544789685Z
tags:
- release
depends_on:
- ea65
---

Run cargo publish. Ensure Cargo.toml has description, license, repository, and keywords fields set. Check for any API-breaking changes and set semver version accordingly.