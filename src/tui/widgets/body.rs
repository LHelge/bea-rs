use ratatui::text::{Line, Span};

use super::super::style::Theme;

pub(in crate::tui) struct BodyWidget<'a> {
    body: &'a str,
    theme: &'a Theme,
}

impl<'a> BodyWidget<'a> {
    pub fn new(body: &'a str, theme: &'a Theme) -> Self {
        Self { body, theme }
    }

    pub fn lines(&self) -> Vec<Line<'a>> {
        if self.body.trim().is_empty() {
            return Vec::new();
        }

        let mut lines = Vec::new();
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "───────────────────────────────",
            ratatui::style::Style::default().fg(self.theme.separator),
        )));
        lines.push(Line::from(""));

        let rendered = tui_markdown::from_str(self.body);
        lines.extend(rendered.lines);

        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect_text(lines: &[Line]) -> String {
        lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect()
    }

    fn theme() -> Theme {
        Theme::default()
    }

    #[test]
    fn blank_body_returns_empty() {
        let t = theme();
        assert!(BodyWidget::new("", &t).lines().is_empty());
        assert!(BodyWidget::new("   ", &t).lines().is_empty());
        assert!(BodyWidget::new("\n\n", &t).lines().is_empty());
    }

    #[test]
    fn body_with_content_has_separator() {
        let t = theme();
        let lines = BodyWidget::new("Hello world", &t).lines();
        assert!(lines.len() >= 4); // blank + separator + blank + content

        let text = collect_text(&lines);
        assert!(text.contains("───"));
        assert!(text.contains("Hello world"));
    }

    #[test]
    fn multiline_body() {
        let t = theme();
        let lines = BodyWidget::new("Line 1\nLine 2\nLine 3", &t).lines();
        let text = collect_text(&lines);
        assert!(text.contains("Line 1"));
        assert!(text.contains("Line 2"));
        assert!(text.contains("Line 3"));
    }

    #[test]
    fn bold_text_is_styled() {
        let t = theme();
        let lines = BodyWidget::new("**bold text**", &t).lines();
        // Find the span containing "bold text" and verify it has bold modifier
        let bold_span = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .find(|s| s.content.contains("bold text"));
        assert!(bold_span.is_some(), "should contain 'bold text' span");
        let style = bold_span.unwrap().style;
        assert!(
            style.add_modifier.contains(ratatui::style::Modifier::BOLD),
            "bold text should have BOLD modifier"
        );
    }

    #[test]
    fn heading_is_rendered() {
        let t = theme();
        let lines = BodyWidget::new("# My Heading", &t).lines();
        let text = collect_text(&lines);
        assert!(text.contains("My Heading"));
    }

    #[test]
    fn code_block_is_rendered() {
        let t = theme();
        let lines = BodyWidget::new("```\nlet x = 1;\n```", &t).lines();
        let text = collect_text(&lines);
        assert!(text.contains("let x = 1;"));
    }
}
