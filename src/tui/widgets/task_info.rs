use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::task::{Task, TaskType};

use super::super::style::Theme;

pub(in crate::tui) struct TaskInfoWidget<'a> {
    task: &'a Task,
    theme: &'a Theme,
}

impl<'a> TaskInfoWidget<'a> {
    pub fn new(task: &'a Task, theme: &'a Theme) -> Self {
        Self { task, theme }
    }

    pub fn lines(&self) -> Vec<Line<'a>> {
        let mut lines = Vec::new();
        let label_style = self.theme.label_style();

        // Title
        lines.push(Line::from(Span::styled(
            self.task.title.as_str(),
            self.theme.title_style(),
        )));
        lines.push(Line::from(""));

        // Status
        lines.push(Line::from(vec![
            Span::styled("Status:   ", label_style),
            Span::raw(self.task.status.to_string()),
        ]));

        // Priority
        lines.push(Line::from(vec![
            Span::styled("Priority: ", label_style),
            Span::styled(
                self.task.priority.to_string(),
                Style::default().fg(self.theme.priority_color(self.task.priority)),
            ),
        ]));

        // Type (only if epic)
        if self.task.task_type == TaskType::Epic {
            lines.push(Line::from(vec![
                Span::styled("Type:     ", label_style),
                Span::raw("epic"),
            ]));
        }

        // Tags
        if !self.task.tags.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("Tags:     ", label_style),
                Span::raw(self.task.tags.join(", ")),
            ]));
        }

        // Assignee
        if !self.task.assignee.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("Assignee: ", label_style),
                Span::raw(self.task.assignee.as_str()),
            ]));
        }

        // Parent
        if let Some(ref parent) = self.task.parent {
            lines.push(Line::from(vec![
                Span::styled("Parent:   ", label_style),
                Span::raw(parent.as_str()),
            ]));
        }

        // Timestamps
        lines.push(Line::from(vec![
            Span::styled("Created:  ", label_style),
            Span::raw(self.task.created.format("%Y-%m-%d %H:%M").to_string()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Updated:  ", label_style),
            Span::raw(self.task.updated.format("%Y-%m-%d %H:%M").to_string()),
        ]));

        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{Priority, Task};

    #[test]
    fn basic_task_info_lines() {
        let task = Task::new("abc".into(), "My Task".into(), Priority::P1);
        let theme = Theme::default();
        let widget = TaskInfoWidget::new(&task, &theme);
        let lines = widget.lines();

        // Title + blank + status + priority + created + updated = 6 minimum
        assert!(lines.len() >= 6);

        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(text.contains("My Task"));
        assert!(text.contains("Status:"));
        assert!(text.contains("Priority:"));
    }

    #[test]
    fn epic_shows_type() {
        let mut task = Task::new("abc".into(), "Epic".into(), Priority::P0);
        task.task_type = TaskType::Epic;
        let theme = Theme::default();
        let widget = TaskInfoWidget::new(&task, &theme);
        let lines = widget.lines();

        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(text.contains("Type:"));
        assert!(text.contains("epic"));
    }

    #[test]
    fn optional_fields_shown_when_present() {
        let mut task = Task::new("abc".into(), "Tagged".into(), Priority::P2);
        task.tags = vec!["backend".into(), "auth".into()];
        task.assignee = "alice".into();
        task.parent = Some("xyz".into());
        let theme = Theme::default();
        let widget = TaskInfoWidget::new(&task, &theme);
        let lines = widget.lines();

        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(text.contains("Tags:"));
        assert!(text.contains("backend, auth"));
        assert!(text.contains("Assignee:"));
        assert!(text.contains("alice"));
        assert!(text.contains("Parent:"));
        assert!(text.contains("xyz"));
    }

    #[test]
    fn optional_fields_hidden_when_absent() {
        let task = Task::new("abc".into(), "Plain".into(), Priority::P3);
        let theme = Theme::default();
        let widget = TaskInfoWidget::new(&task, &theme);
        let lines = widget.lines();

        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(!text.contains("Tags:"));
        assert!(!text.contains("Assignee:"));
        assert!(!text.contains("Parent:"));
        assert!(!text.contains("Type:"));
    }
}
