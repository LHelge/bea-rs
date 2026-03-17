use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use super::super::app::{Filter, FocusPane, Mode};

pub(in crate::tui) struct BottomBarWidget<'a> {
    mode: &'a Mode,
    focus: &'a FocusPane,
    filter: &'a Filter,
    error_message: Option<&'a str>,
}

impl<'a> BottomBarWidget<'a> {
    pub fn new(
        mode: &'a Mode,
        focus: &'a FocusPane,
        filter: &'a Filter,
        error_message: Option<&'a str>,
    ) -> Self {
        Self {
            mode,
            focus,
            filter,
            error_message,
        }
    }
}

impl Widget for BottomBarWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Show error message if present
        if let Some(msg) = self.error_message {
            let bar = Paragraph::new(Line::from(vec![
                Span::styled(" ERROR ", Style::default().fg(Color::White).bg(Color::Red)),
                Span::styled(format!(" {msg} "), Style::default().fg(Color::Red)),
            ]))
            .style(Style::default().bg(Color::Black));
            bar.render(area, buf);
            return;
        }

        let hints: Vec<(&str, &str)> = match self.mode {
            Mode::Normal => {
                let nav_hint = match self.focus {
                    FocusPane::List => "navigate",
                    FocusPane::Detail => "scroll",
                };
                vec![
                    ("q", "quit"),
                    ("Tab", "switch pane"),
                    ("j/k", nav_hint),
                    ("e", "edit"),
                    ("c", "create"),
                    ("s", "status"),
                    ("d", "delete"),
                    ("/", "filter"),
                    (
                        "a",
                        if self.filter.show_all {
                            "hide done"
                        } else {
                            "show all"
                        },
                    ),
                ]
            }
            Mode::StatusSelect { .. } => {
                vec![("Esc", "cancel"), ("j/k", "navigate"), ("Enter", "confirm")]
            }
            Mode::CreateInput { .. } => vec![("Esc", "cancel"), ("Enter", "create")],
            Mode::FilterInput { .. } => vec![("Esc", "cancel"), ("Enter", "apply")],
            Mode::ConfirmDelete { .. } => vec![("y", "confirm delete"), ("any", "cancel")],
        };

        // Build hint spans, truncating when terminal width is exceeded
        let max_width = area.width as usize;
        let mut spans: Vec<Span> = Vec::new();
        let mut used_width: usize = 0;

        for (i, (key, desc)) in hints.iter().enumerate() {
            let key_text = format!(" {key} ");
            let desc_text = format!(" {desc} ");
            let sep_width = if i < hints.len() - 1 { 1 } else { 0 };
            let entry_width = key_text.len() + desc_text.len() + sep_width;

            if used_width + entry_width > max_width {
                break;
            }

            spans.push(Span::styled(
                key_text,
                Style::default().fg(Color::Black).bg(Color::White),
            ));
            spans.push(Span::styled(
                desc_text,
                Style::default().fg(Color::DarkGray),
            ));
            if i < hints.len() - 1 {
                spans.push(Span::raw(" "));
            }
            used_width += entry_width;
        }

        let bar = Paragraph::new(Line::from(spans)).style(Style::default().bg(Color::Black));
        bar.render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_mode_shows_quit_hint() {
        let mode = Mode::Normal;
        let focus = FocusPane::List;
        let filter = Filter::default();
        let widget = BottomBarWidget::new(&mode, &focus, &filter, None);

        let area = Rect::new(0, 0, 80, 1);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        let text: String = buf.content().iter().map(|c| c.symbol()).collect::<String>();
        assert!(text.contains("quit"));
        assert!(text.contains("navigate"));
    }

    #[test]
    fn detail_focus_shows_scroll() {
        let mode = Mode::Normal;
        let focus = FocusPane::Detail;
        let filter = Filter::default();
        let widget = BottomBarWidget::new(&mode, &focus, &filter, None);

        let area = Rect::new(0, 0, 80, 1);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        let text: String = buf.content().iter().map(|c| c.symbol()).collect::<String>();
        assert!(text.contains("scroll"));
    }

    #[test]
    fn error_message_displayed() {
        let mode = Mode::Normal;
        let focus = FocusPane::List;
        let filter = Filter::default();
        let widget = BottomBarWidget::new(&mode, &focus, &filter, Some("something broke"));

        let area = Rect::new(0, 0, 80, 1);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        let text: String = buf.content().iter().map(|c| c.symbol()).collect::<String>();
        assert!(text.contains("ERROR"));
        assert!(text.contains("something broke"));
    }
}
