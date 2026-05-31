---
id: pg6
title: Fix UTF-8 byte-slice panic in TUI input modal
status: done
priority: P1
created: 2026-05-30T21:38:10.772804619Z
updated: 2026-05-30T23:07:29.758621300Z
tags:
- bug
- tui
- panic
parent: nya
---

`render_text_box` in `src/tui/widgets/modals.rs:121-125` truncates with byte slicing:

```rust
let start = input.len() - tail_len;
format!("…{}", &input[start..])  // panics if start isn't a char boundary
```

`input.len()`/`start` are byte offsets, but the create/filter input accepts any `KeyCode::Char(c)` (`src/tui/input.rs:28-32`), so the buffer can hold multi-byte UTF-8. A long title containing an accented char or emoji slices mid-codepoint → panic, crashing the TUI mid-session (and possibly leaving the terminal in raw mode).

Fix: truncate by `chars()` (take the last `inner_width-1` chars) instead of byte slicing. Add a regression test with a multibyte input string.