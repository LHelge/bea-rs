use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

pub(in crate::tui) struct BodyWidget<'a> {
    body: &'a str,
}

impl<'a> BodyWidget<'a> {
    pub fn new(body: &'a str) -> Self {
        Self { body }
    }

    pub fn lines(&self) -> Vec<Line<'a>> {
        if self.body.trim().is_empty() {
            return Vec::new();
        }

        let mut lines = Vec::new();
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "───────────────────────────────",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(""));

        for line in self.body.lines() {
            lines.push(Line::from(line.to_string()));
        }

        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_body_returns_empty() {
        assert!(BodyWidget::new("").lines().is_empty());
        assert!(BodyWidget::new("   ").lines().is_empty());
        assert!(BodyWidget::new("\n\n").lines().is_empty());
    }

    #[test]
    fn body_with_content_has_separator() {
        let lines = BodyWidget::new("Hello world").lines();
        assert!(lines.len() >= 4); // blank + separator + blank + content

        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(text.contains("───"));
        assert!(text.contains("Hello world"));
    }

    #[test]
    fn multiline_body() {
        let lines = BodyWidget::new("Line 1\nLine 2\nLine 3").lines();
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(text.contains("Line 1"));
        assert!(text.contains("Line 2"));
        assert!(text.contains("Line 3"));
    }
}
