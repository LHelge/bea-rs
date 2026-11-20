use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, StatefulWidget};

use bears::task::{Task, TaskType};

use super::super::app::Filter;
use super::super::style::{self, Theme};

pub(in crate::tui) struct TaskListWidget<'a> {
    tasks: &'a [Task],
    filter: &'a Filter,
    border_style: Style,
    theme: &'a Theme,
}

impl<'a> TaskListWidget<'a> {
    pub fn new(
        tasks: &'a [Task],
        filter: &'a Filter,
        border_style: Style,
        theme: &'a Theme,
    ) -> Self {
        Self {
            tasks,
            filter,
            border_style,
            theme,
        }
    }
}

impl StatefulWidget for TaskListWidget<'_> {
    type State = ListState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut ListState) {
        // Dotted IDs vary in length; pad each to the longest so titles line up.
        let id_width = self
            .tasks
            .iter()
            .map(|t| t.id.chars().count())
            .max()
            .unwrap_or(0);
        let items: Vec<ListItem> = self
            .tasks
            .iter()
            .map(|task| {
                let pri_color = self.theme.priority_color(task.priority);
                let indicator = style::status_indicator(&task.status);
                let st_color = self.theme.status_color(&task.status);
                let type_prefix = if task.task_type == TaskType::Epic {
                    "◆ "
                } else {
                    ""
                };

                let line = Line::from(vec![
                    Span::styled(
                        format!("{} ", task.priority),
                        Style::default().fg(pri_color),
                    ),
                    Span::styled(format!("{indicator} "), Style::default().fg(st_color)),
                    Span::styled(
                        format!("{:<w$} ", format!("[{}]", task.id), w = id_width + 2),
                        Style::default().fg(self.theme.id_color),
                    ),
                    Span::raw(format!("{type_prefix}{}", task.title)),
                ]);

                ListItem::new(line)
            })
            .collect();

        let title = {
            let mut parts = vec![" Tasks".to_string()];
            if !self.filter.query.is_empty() {
                parts.push(format!(" [{}]", self.filter.query));
            }
            parts.push(format!(" ({})", self.filter.list_mode.label()));
            parts.push(format!(" ({}) ", self.tasks.len()));
            parts.join("")
        };

        let list = List::new(items)
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_style(self.border_style),
            )
            .highlight_style(self.theme.highlight_style())
            .highlight_symbol(style::HIGHLIGHT_SYMBOL);

        StatefulWidget::render(list, area, buf, state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bears::task::{Priority, Task};
    use ratatui::buffer::Buffer;

    fn sample_tasks() -> Vec<Task> {
        vec![
            Task::new("aaa".into(), "Task A".into(), Priority::P1),
            Task::new("bbb".into(), "Task B".into(), Priority::P2),
        ]
    }

    #[test]
    fn renders_task_titles() {
        let tasks = sample_tasks();
        let filter = Filter::default();
        let theme = Theme::default();
        let widget = TaskListWidget::new(&tasks, &filter, Style::default(), &theme);

        let area = Rect::new(0, 0, 40, 10);
        let mut buf = Buffer::empty(area);
        let mut state = ListState::default();
        StatefulWidget::render(widget, area, &mut buf, &mut state);

        let text: String = buf.content().iter().map(|c| c.symbol()).collect::<String>();
        assert!(text.contains("Task A"));
        assert!(text.contains("Task B"));
    }

    #[test]
    fn filter_title_shows_query() {
        let tasks = sample_tasks();
        let filter = Filter {
            query: "test".into(),
            ..Filter::default()
        };
        let theme = Theme::default();
        let widget = TaskListWidget::new(&tasks, &filter, Style::default(), &theme);

        let area = Rect::new(0, 0, 40, 10);
        let mut buf = Buffer::empty(area);
        let mut state = ListState::default();
        StatefulWidget::render(widget, area, &mut buf, &mut state);

        let text: String = buf.content().iter().map(|c| c.symbol()).collect::<String>();
        assert!(text.contains("[test]"));
    }

    #[test]
    fn shows_epic_marker() {
        let mut task = Task::new("eee".into(), "My Epic".into(), Priority::P0);
        task.task_type = TaskType::Epic;
        let tasks = vec![task];
        let filter = Filter::default();
        let theme = Theme::default();
        let widget = TaskListWidget::new(&tasks, &filter, Style::default(), &theme);

        let area = Rect::new(0, 0, 50, 5);
        let mut buf = Buffer::empty(area);
        let mut state = ListState::default();
        StatefulWidget::render(widget, area, &mut buf, &mut state);

        let text: String = buf.content().iter().map(|c| c.symbol()).collect::<String>();
        assert!(text.contains("◆"));
        assert!(text.contains("My Epic"));
    }

    #[test]
    fn titles_line_up_across_id_lengths() {
        let tasks = vec![
            Task::new("a1b".into(), "Root".into(), Priority::P1),
            Task::new("a1b.c2.d7".into(), "Grandchild".into(), Priority::P1),
        ];
        let filter = Filter::default();
        let theme = Theme::default();
        let widget = TaskListWidget::new(&tasks, &filter, Style::default(), &theme);

        let area = Rect::new(0, 0, 40, 5);
        let mut buf = Buffer::empty(area);
        let mut state = ListState::default();
        StatefulWidget::render(widget, area, &mut buf, &mut state);

        let row = |y: u16| -> String { (0..area.width).map(|x| buf[(x, y)].symbol()).collect() };
        let (first, second) = (row(1), row(2));
        assert_eq!(
            first.find("Root"),
            second.find("Grandchild"),
            "titles should start in the same column:\n{first}\n{second}"
        );
    }
}
