---
id: qm8
title: "bea snapshot -o: refuse a path inside .bears/ and write atomically"
status: done
priority: P3
created: "2026-11-20T21:04:07.529787703Z"
updated: "2026-11-20T22:27:06.078561592Z"
tags:
  - snapshot
  - cli
attempts: 1
---

Review notes on bjt.r9 (`cmd_snapshot` in `src/cli/cmd.rs`). (1) `bea snapshot -o .bears/snap.md` is accepted; afterwards every loading command warns "skipping .bears/snap.md: invalid frontmatter" and `bea doctor` exits 1 until the file is deleted. Refuse an output path inside `.bears/`. (2) The file is written in place with `std::fs::write`; a job killed mid-write leaves a truncated snapshot. Write to a temporary file beside it and rename.