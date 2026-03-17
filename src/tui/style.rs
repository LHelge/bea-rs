use ratatui::style::Color;

use crate::task::Status;

pub(super) const ALL_STATUSES: &[Status] = &[
    Status::Open,
    Status::InProgress,
    Status::Done,
    Status::Blocked,
    Status::Cancelled,
];

pub(super) fn priority_color(p: crate::task::Priority) -> Color {
    match p {
        crate::task::Priority::P0 => Color::Red,
        crate::task::Priority::P1 => Color::Yellow,
        crate::task::Priority::P2 => Color::Blue,
        crate::task::Priority::P3 => Color::DarkGray,
    }
}

pub(super) fn status_indicator(s: &Status) -> &'static str {
    match s {
        Status::Open => "○",
        Status::InProgress => "●",
        Status::Done => "✓",
        Status::Blocked => "⊘",
        Status::Cancelled => "✗",
    }
}
