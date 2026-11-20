---
id: gyj
title: Give store::write_task_file a unique temporary file name
status: open
priority: P3
created: "2026-11-20T22:27:34.202308296Z"
updated: "2026-11-20T22:27:34.202308296Z"
tags:
  - core
---

Found doing qm8. `store::write_task_file` writes through a fixed temporary name (`.{name}.tmp`) and does not remove it when the write fails. Two processes writing the same task at once (two agents, or an agent and the TUI) can clash on that file, and a failed write leaves it behind. `write_atomically` in `src/cli/cmd.rs` (added for `bea snapshot -o`) uses a unique name with `create_new` and cleans up; consider moving that into the core and using it for both.