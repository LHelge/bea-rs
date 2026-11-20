---
id: zsf.e4
title: Replace serde_yml with a maintained YAML crate
status: done
priority: P1
created: "2026-11-20T12:52:25.699948574Z"
updated: "2026-11-20T13:11:22.386105087Z"
tags:
  - deps
depends_on:
  - zsf.m4
attempts: 1
---

First verify the status: serde_yml is believed to be unmaintained with a RustSec unsoundness advisory (unverified). If confirmed, swap to a maintained fork (serde_yaml_ng or serde_norway). Usage is confined to task.rs, store.rs and config.rs. Frontmatter output must stay byte-compatible where possible; add a round-trip test over the existing .bears files.