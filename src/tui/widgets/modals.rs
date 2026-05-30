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

/// Truncate `input` to fit within `inner_width` columns, using character-based counting.
///
/// If the input exceeds `inner_width` characters, a `…` prefix is prepended and
/// the last `inner_width - 1` characters of `input` are shown. This avoids byte-slicing
/// mid-codepoint panics for multi-byte UTF-8 input (accented chars, emoji, etc.).
pub(in crate::tui) fn truncate_input_for_display(input: &str, inner_width: usize) -> String {
    let char_count = input.chars().count();
    if char_count > inner_width && inner_width > 1 {
        let tail_len = inner_width - 1; // one column reserved for the '…' prefix
        let tail: String = input
            .chars()
            .rev()
            .take(tail_len)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        format!("…{tail}")
    } else {
        input.to_string()
    }
}

/// Shared helper: clear area, render a bordered text box with optional input scrolling.
fn render_text_box(buf: &mut Buffer, area: Rect, title: &str, input: &str, theme: &Theme) {
    Clear.render(area, buf);

    let inner_width = area.width.saturating_sub(2) as usize;
    let display_text = truncate_input_for_display(input, inner_width);

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

#[cfg(test)]
mod tests {
    use super::truncate_input_for_display;

    #[test]
    fn short_input_returned_unchanged() {
        assert_eq!(truncate_input_for_display("hello", 10), "hello");
    }

    #[test]
    fn exact_fit_not_truncated() {
        assert_eq!(truncate_input_for_display("hello", 5), "hello");
    }

    #[test]
    fn ascii_truncation_adds_ellipsis() {
        // "abcdefghij" is 10 chars, inner_width=5 → "…fghij" (tail_len=4 → "…ghij")
        let result = truncate_input_for_display("abcdefghij", 5);
        assert!(result.starts_with('…'), "should start with ellipsis");
        assert_eq!(
            result.chars().count(),
            5,
            "should be exactly inner_width chars"
        );
    }

    /// Regression: multi-byte UTF-8 input must not panic and must display correctly.
    #[test]
    fn multibyte_utf8_no_panic_and_correct_tail() {
        // Each of these chars is 2–4 bytes in UTF-8; byte-slicing at a computed
        // byte offset would panic with 'byte index N is not a char boundary'.
        let input = "café naïve résumé 🦀 done";
        let inner_width = 8;
        // Must not panic
        let result = truncate_input_for_display(input, inner_width);
        // Result must fit within inner_width chars and start with '…'
        assert!(
            result.starts_with('…'),
            "result should start with ellipsis: {result:?}"
        );
        assert_eq!(
            result.chars().count(),
            inner_width,
            "result should be exactly inner_width chars: {result:?}"
        );
        // The tail should be the last (inner_width - 1) chars of input
        let expected_tail: String = input
            .chars()
            .rev()
            .take(inner_width - 1)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        assert!(
            result.ends_with(&expected_tail),
            "result {result:?} should end with {expected_tail:?}"
        );
    }

    /// Emoji input: single emoji is 1 char (multiple bytes); truncation must not panic.
    #[test]
    fn emoji_only_input_no_panic() {
        let input = "🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀"; // 10 crabs, each 4 bytes
        let result = truncate_input_for_display(input, 5);
        assert!(result.starts_with('…'));
        assert_eq!(result.chars().count(), 5);
    }
}
