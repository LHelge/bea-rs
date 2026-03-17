use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, StatefulWidget};

use crate::task::{Task, TaskType};

use super::super::app::Filter;
use super::super::style::{priority_color, status_indicator};

pub(in crate::tui) struct TaskListWidget<'a> {
    tasks: &'a [Task],
    filter: &'a Filter,
    border_style: Style,
}

impl<'a> TaskListWidget<'a> {
    pub fn new(tasks: &'a [Task], filter: &'a Filter, border_style: Style) -> Self {
        Self {
            tasks,
            filter,
            border_style,
        }
    }
}

impl StatefulWidget for TaskListWidget<'_> {
    type State = ListState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut ListState) {
        let items: Vec<ListItem> = self
            .tasks
            .iter()
            .map(|task| {
                let pri_color = priority_color(task.priority);
                let indicator = status_indicator(&task.status);
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
                    Span::styled(format!("{indicator} "), Style::default().fg(Color::White)),
                    Span::styled(
                        format!("[{}] ", task.id),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::raw(format!("{type_prefix}{}", task.title)),
                ]);

                ListItem::new(line)
            })
            .collect();

        let title = if self.filter.is_active() {
            let mut parts = vec![" Tasks".to_string()];
            if !self.filter.query.is_empty() {
                parts.push(format!(" [{}]", self.filter.query));
            }
            if self.filter.show_all {
                parts.push(" (all)".to_string());
            }
            parts.push(format!(" ({}) ", self.tasks.len()));
            parts.join("")
        } else {
            format!(" Tasks ({}) ", self.tasks.len())
        };

        let list = List::new(items)
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_style(self.border_style),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");

        StatefulWidget::render(list, area, buf, state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{Priority, Task};
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
        let widget = TaskListWidget::new(&tasks, &filter, Style::default());

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
            show_all: false,
        };
        let widget = TaskListWidget::new(&tasks, &filter, Style::default());

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
        let widget = TaskListWidget::new(&tasks, &filter, Style::default());

        let area = Rect::new(0, 0, 50, 5);
        let mut buf = Buffer::empty(area);
        let mut state = ListState::default();
        StatefulWidget::render(widget, area, &mut buf, &mut state);

        let text: String = buf.content().iter().map(|c| c.symbol()).collect::<String>();
        assert!(text.contains("◆"));
        assert!(text.contains("My Epic"));
    }
}
