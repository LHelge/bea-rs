use ratatui::style::{Color, Modifier, Style};

use crate::task::{Priority, Status};

// ── Status indicators ────────────────────────────────────────────────────

pub(super) const ALL_STATUSES: &[Status] = &[
    Status::Open,
    Status::InProgress,
    Status::Done,
    Status::Blocked,
    Status::Cancelled,
];

pub(super) fn status_indicator(s: &Status) -> &'static str {
    match s {
        Status::Open => "○",
        Status::InProgress => "●",
        Status::Done => "✓",
        Status::Blocked => "⊘",
        Status::Cancelled => "✗",
    }
}

/// Highlight symbol shown next to selected row.
pub(super) const HIGHLIGHT_SYMBOL: &str = "▶ ";

// ── Theme ────────────────────────────────────────────────────────────────

/// Central color/style theme for the TUI.
///
/// All rendering code reads colors from a shared `&Theme` reference.
/// Override `Default` to create custom palettes (e.g. loaded from a file).
pub(super) struct Theme {
    // Borders
    pub border_focused: Color,
    pub border_unfocused: Color,

    // Highlight (selected row)
    pub highlight_bg: Color,

    // Labels & headings
    pub label: Color,
    pub section_heading: Color,
    pub title_fg: Color,

    // Separator
    pub separator: Color,

    // Bottom bar
    pub bar_key_fg: Color,
    pub bar_key_bg: Color,
    pub bar_desc_fg: Color,
    pub bar_bg: Color,
    pub bar_error_fg: Color,
    pub bar_error_bg: Color,
    pub bar_error_msg: Color,

    // Modals
    pub modal_bg: Color,

    // Task list
    pub id_color: Color,

    // Status colors
    pub status_open: Color,
    pub status_in_progress: Color,
    pub status_done: Color,
    pub status_blocked: Color,
    pub status_cancelled: Color,

    // Priority colors
    pub priority_p0: Color,
    pub priority_p1: Color,
    pub priority_p2: Color,
    pub priority_p3: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            border_focused: Color::LightCyan,
            border_unfocused: Color::DarkGray,

            highlight_bg: Color::Indexed(236),

            label: Color::LightBlue,
            section_heading: Color::LightBlue,
            title_fg: Color::White,

            separator: Color::DarkGray,

            bar_key_fg: Color::Indexed(234),
            bar_key_bg: Color::LightCyan,
            bar_desc_fg: Color::Gray,
            bar_bg: Color::Indexed(235),
            bar_error_fg: Color::White,
            bar_error_bg: Color::Red,
            bar_error_msg: Color::LightRed,

            modal_bg: Color::Indexed(236),

            id_color: Color::DarkGray,

            status_open: Color::Cyan,
            status_in_progress: Color::LightYellow,
            status_done: Color::LightGreen,
            status_blocked: Color::LightMagenta,
            status_cancelled: Color::DarkGray,

            priority_p0: Color::LightRed,
            priority_p1: Color::LightYellow,
            priority_p2: Color::LightCyan,
            priority_p3: Color::DarkGray,
        }
    }
}

impl Theme {
    pub fn status_color(&self, s: &Status) -> Color {
        match s {
            Status::Open => self.status_open,
            Status::InProgress => self.status_in_progress,
            Status::Done => self.status_done,
            Status::Blocked => self.status_blocked,
            Status::Cancelled => self.status_cancelled,
        }
    }

    pub fn priority_color(&self, p: Priority) -> Color {
        match p {
            Priority::P0 => self.priority_p0,
            Priority::P1 => self.priority_p1,
            Priority::P2 => self.priority_p2,
            Priority::P3 => self.priority_p3,
        }
    }

    pub fn highlight_style(&self) -> Style {
        Style::default()
            .bg(self.highlight_bg)
            .add_modifier(Modifier::BOLD)
    }

    pub fn label_style(&self) -> Style {
        Style::default().fg(self.label)
    }

    pub fn title_style(&self) -> Style {
        Style::default()
            .fg(self.title_fg)
            .add_modifier(Modifier::BOLD)
    }

    pub fn section_heading_style(&self) -> Style {
        Style::default()
            .fg(self.section_heading)
            .add_modifier(Modifier::BOLD)
    }

    pub fn border_style(&self, focused: bool) -> Style {
        if focused {
            Style::default().fg(self.border_focused)
        } else {
            Style::default().fg(self.border_unfocused)
        }
    }
}
