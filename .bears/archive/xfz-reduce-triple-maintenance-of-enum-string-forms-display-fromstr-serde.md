---
id: xfz
title: Reduce triple-maintenance of enum string forms (Display/FromStr/serde)
status: done
priority: P3
created: 2026-05-30T21:39:01.766587596Z
updated: 2026-05-30T22:53:33.857229949Z
tags:
- cleanup
- idiomatic
parent: nya
---

`Status`, `Priority`, and `TaskType` (`src/task.rs`) each maintain `Display` + `FromStr` + serde `rename_all`/variant names separately. Adding a variant means editing three spots that must stay in sync. Consider driving `FromStr` off serde (or a small macro / strum-style helper) so the string forms have a single source of truth. Keep the dependency footprint small per project guidelines.