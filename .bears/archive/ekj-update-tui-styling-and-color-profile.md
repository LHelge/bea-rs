---
id: ekj
title: Update TUI styling and color profile
status: done
priority: P2
created: 2026-03-17T20:56:53.455814915Z
updated: 2026-03-17T21:03:12.270740004Z
tags:
- tui
- design
---

# TUI Styling and Color Profile Update

Redesign the TUI color scheme for better readability and visual consistency.

## Goals

- Improve contrast in **dark** and **light** terminal themes
- Use *semantic colors* for task states
- Add `highlight` styles for focused elements

## Current Issues

1. Status colors are hard to distinguish on some terminals
2. Priority indicators lack visual weight
3. The detail panel has no **bold** or *italic* differentiation

## Proposed Color Mapping

| Element       | Current    | Proposed     |
|---------------|------------|--------------|
| `open`        | White      | **Cyan**     |
| `in_progress` | Yellow     | **Blue**     |
| `done`        | Green      | **Green**    |
| `blocked`     | Red        | **Magenta**  |
| `cancelled`   | DarkGray   | **DarkGray** |

## Implementation Plan

### 1. Extract color constants

Move all color definitions into a `theme` module:

```rust
pub struct Theme {
    pub status_open: Color,
    pub status_in_progress: Color,
    pub status_done: Color,
    pub border_focused: Color,
    pub border_unfocused: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            status_open: Color::Cyan,
            status_in_progress: Color::Blue,
            status_done: Color::Green,
            border_focused: Color::Yellow,
            border_unfocused: Color::DarkGray,
        }
    }
}
```

### 2. Update widget styles

Replace hardcoded `Color::` references with theme lookups:

```rust
let style = match task.status {
    Status::Open => theme.status_open,
    Status::InProgress => theme.status_in_progress,
    Status::Done => theme.status_done,
    _ => Color::White,
};
```

### 3. Test with different terminals

- [ ] Alacritty (dark)
- [ ] Alacritty (light)
- [ ] Kitty
- [ ] macOS Terminal

## Notes

> This should be done carefully to avoid breaking the existing
> visual hierarchy. Test with real task data before merging.

See also the `owo-colors` crate used in CLI output — keep the two consistent where possible.