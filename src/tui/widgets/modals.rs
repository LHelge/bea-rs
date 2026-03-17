use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{
    Block, Borders, Clear, List, ListItem, ListState, Paragraph, StatefulWidget, Widget,
};

use super::super::style::{ALL_STATUSES, Theme};

/// Status selection modal rendered centered on screen.
pub(in crate::tui) struct StatusModalWidget<'a> {
    pub selected: usize,
    pub theme: &'a Theme,
}

impl<'a> StatusModalWidget<'a> {
    pub fn new(selected: usize, theme: &'a Theme) -> Self {
        Self { selected, theme }
    }
}

impl Widget for StatusModalWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let modal_width = 30u16;
        let modal_height = (ALL_STATUSES.len() as u16) + 2;
        let x = area.x + (area.width.saturating_sub(modal_width)) / 2;
        let y = area.y + (area.height.saturating_sub(modal_height)) / 2;
        let modal_area = Rect::new(x, y, modal_width, modal_height);

        Clear.render(modal_area, buf);

        let items: Vec<ListItem> = ALL_STATUSES
            .iter()
            .enumerate()
            .map(|(i, status)| {
                let marker = if i == self.selected { "▶ " } else { "  " };
                ListItem::new(format!("{marker}{status}"))
            })
            .collect();

        let mut modal_list_state = ListState::default();
        modal_list_state.select(Some(self.selected));

        let list = List::new(items)
            .block(
                Block::default()
                    .title(" Set Status ")
                    .borders(Borders::ALL)
                    .style(Style::default().bg(self.theme.modal_bg)),
            )
            .highlight_style(self.theme.highlight_style());

        StatefulWidget::render(list, modal_area, buf, &mut modal_list_state);
    }
}

/// Centered text-input modal.
pub(in crate::tui) struct InputModalWidget<'a> {
    pub title: &'a str,
    pub input: &'a str,
    pub theme: &'a Theme,
}

impl<'a> InputModalWidget<'a> {
    pub fn new(title: &'a str, input: &'a str, theme: &'a Theme) -> Self {
        Self {
            title,
            input,
            theme,
        }
    }
}

impl Widget for InputModalWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let modal_width = (area.width / 2).max(30).min(area.width);
        let modal_height = 3u16;
        let x = area.x + (area.width.saturating_sub(modal_width)) / 2;
        let y = area.y + (area.height.saturating_sub(modal_height)) / 2;
        let modal_area = Rect::new(x, y, modal_width, modal_height);

        render_text_box(buf, modal_area, self.title, self.input, self.theme);
    }
}

/// Bottom-anchored text-input prompt.
pub(in crate::tui) struct InputPromptWidget<'a> {
    pub title: &'a str,
    pub input: &'a str,
    pub theme: &'a Theme,
}

impl<'a> InputPromptWidget<'a> {
    pub fn new(title: &'a str, input: &'a str, theme: &'a Theme) -> Self {
        Self {
            title,
            input,
            theme,
        }
    }
}

impl Widget for InputPromptWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let input_area = Rect::new(
            area.x,
            area.y + area.height.saturating_sub(3),
            area.width,
            3,
        );

        render_text_box(buf, input_area, self.title, self.input, self.theme);
    }
}

/// Shared helper: clear area, render a bordered text box with optional input scrolling.
fn render_text_box(buf: &mut Buffer, area: Rect, title: &str, input: &str, theme: &Theme) {
    Clear.render(area, buf);

    let inner_width = area.width.saturating_sub(2) as usize;
    let display_text = if input.len() > inner_width && inner_width > 1 {
        let tail_len = inner_width - 1;
        let start = input.len() - tail_len;
        format!("…{}", &input[start..])
    } else {
        input.to_string()
    };

    Paragraph::new(display_text)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .style(Style::default().bg(theme.modal_bg)),
        )
        .style(Style::default().fg(theme.title_fg))
        .render(area, buf);
}
