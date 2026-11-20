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

/// Centered, scrollable modal listing the tasks an operation gave new IDs
/// (`old → new`, one per line, in the order given).
///
/// Shown after deleting an epic detaches its children, so the user can read
/// every rename however many there are: rows that do not fit are scrolled
/// to, and the title says which rows are on screen.
pub(in crate::tui) struct RenamesModalWidget<'a> {
    pub renamed: &'a [(String, String)],
    pub scroll: usize,
    pub theme: &'a Theme,
}

/// Lines above the list inside the renames modal's border.
const RENAMES_INTRO: &str = "These tasks got new IDs:";

impl<'a> RenamesModalWidget<'a> {
    pub fn new(renamed: &'a [(String, String)], scroll: usize, theme: &'a Theme) -> Self {
        Self {
            renamed,
            scroll,
            theme,
        }
    }

    /// The modal's rectangle within `area`: wide enough for the longest
    /// line, tall enough for every row, both capped at the area.
    fn modal_area(&self, area: Rect) -> Rect {
        let widest = self
            .renamed
            .iter()
            .map(|(old, new)| rename_line(old, new).chars().count())
            .chain([RENAMES_INTRO.chars().count(), 30])
            .max()
            .unwrap_or(0);
        let width = (widest as u16).saturating_add(4).min(area.width);
        // Border (2) + intro line + rows.
        let height = (self.renamed.len() as u16)
            .saturating_add(3)
            .min(area.height);
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        Rect::new(x, y, width, height)
    }

    /// How many rename rows fit on screen when the modal is drawn in `area`.
    pub fn visible_rows(&self, area: Rect) -> usize {
        self.modal_area(area).height.saturating_sub(3) as usize
    }

    /// The largest useful scroll offset when drawn in `area`.
    pub fn max_scroll(&self, area: Rect) -> usize {
        self.renamed.len().saturating_sub(self.visible_rows(area))
    }
}

fn rename_line(old: &str, new: &str) -> String {
    format!("{old} → {new}")
}

impl Widget for RenamesModalWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let modal_area = self.modal_area(area);
        let visible = self.visible_rows(area);
        let scroll = self.scroll.min(self.max_scroll(area));
        let total = self.renamed.len();

        let title = if visible >= total {
            format!(
                " Renamed {total} task{} ",
                if total == 1 { "" } else { "s" }
            )
        } else {
            let last = (scroll + visible).min(total);
            format!(" Renamed {total} tasks ({}-{last} of {total}) ", scroll + 1)
        };

        let mut lines = vec![RENAMES_INTRO.to_string()];
        lines.extend(
            self.renamed
                .iter()
                .skip(scroll)
                .take(visible)
                .map(|(old, new)| rename_line(old, new)),
        );

        Clear.render(modal_area, buf);
        Paragraph::new(lines.join("\n"))
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .style(Style::default().bg(self.theme.modal_bg)),
            )
            .style(Style::default().fg(self.theme.title_fg))
            .render(modal_area, buf);
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
    use super::{RenamesModalWidget, Theme};
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::widgets::Widget;

    fn renames(n: usize) -> Vec<(String, String)> {
        (0..n)
            .map(|i| (format!("ep.c{i:02}"), format!("r{i:02}")))
            .collect()
    }

    fn render_renames(renamed: &[(String, String)], scroll: usize, area: Rect) -> String {
        let theme = Theme::default();
        let mut buf = Buffer::empty(area);
        RenamesModalWidget::new(renamed, scroll, &theme).render(area, &mut buf);
        (0..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn renames_modal_shows_every_rename_that_fits() {
        let renamed = renames(3);
        let text = render_renames(&renamed, 0, Rect::new(0, 0, 80, 24));
        assert!(text.contains("Renamed 3 tasks"), "{text}");
        for (old, new) in &renamed {
            assert!(text.contains(&format!("{old} → {new}")), "{text}");
        }
    }

    #[test]
    fn renames_modal_singular_title() {
        let text = render_renames(&renames(1), 0, Rect::new(0, 0, 80, 24));
        assert!(text.contains("Renamed 1 task "), "{text}");
    }

    #[test]
    fn renames_modal_scrolls_when_rows_do_not_fit() {
        let renamed = renames(30);
        let area = Rect::new(0, 0, 80, 10);
        let theme = Theme::default();
        let widget = RenamesModalWidget::new(&renamed, 0, &theme);
        // 10 rows minus border and intro line.
        assert_eq!(widget.visible_rows(area), 7);
        assert_eq!(widget.max_scroll(area), 23);

        let top = render_renames(&renamed, 0, area);
        assert!(top.contains("(1-7 of 30)"), "{top}");
        assert!(top.contains("ep.c00 → r00") && top.contains("ep.c06 → r06"));
        assert!(!top.contains("ep.c07"), "{top}");

        // Scrolling past the end shows the last page, so every row is reachable.
        let bottom = render_renames(&renamed, 99, area);
        assert!(bottom.contains("(24-30 of 30)"), "{bottom}");
        assert!(bottom.contains("ep.c29 → r29"), "{bottom}");
        assert!(!bottom.contains("ep.c22"), "{bottom}");
    }

    #[test]
    fn renames_modal_fits_a_tiny_terminal() {
        // Must not panic when the area is smaller than the modal wants.
        let text = render_renames(&renames(5), 3, Rect::new(0, 0, 10, 2));
        assert!(!text.is_empty());
    }

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
