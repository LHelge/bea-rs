---
id: sak
title: Replace or re-evaluate chrono (RUSTSEC-2026-0543, unmaintained)
status: open
priority: P2
created: "2026-11-20T13:22:09.655234515Z"
updated: "2026-11-20T13:22:09.655234515Z"
tags:
  - deps
---

Found during the dependency refresh epic (zsf). cargo audit flags chrono 0.4.45 as unmaintained with no patched version; it is ignored by id in `.cargo/audit.toml`. chrono is a direct dependency (timestamps in task.rs) and also comes in through rmcp and schemars. Read the advisory first. Options: move bears' own use to jiff or time (the on-disk timestamp format must stay readable and unchanged), and check whether rmcp/schemars pull chrono only through features that can be turned off. Remove the ignore once chrono is out of the lockfile, or record why it stays.