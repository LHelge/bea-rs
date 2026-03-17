use std::collections::HashMap;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::graph::{DepNode, Graph};
use crate::task::{Status, Task};

use super::super::style::status_indicator;

pub(in crate::tui) struct DepTreeWidget<'a> {
    task: &'a Task,
    graph: &'a Graph,
    task_map: &'a HashMap<String, Task>,
}

impl<'a> DepTreeWidget<'a> {
    pub fn new(task: &'a Task, graph: &'a Graph, task_map: &'a HashMap<String, Task>) -> Self {
        Self {
            task,
            graph,
            task_map,
        }
    }

    pub fn lines(&self) -> Vec<Line<'a>> {
        if self.task.depends_on.is_empty() {
            return Vec::new();
        }

        let mut lines = Vec::new();
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Dependencies",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )));

        if let Some(tree) = self.graph.dep_tree(self.task_map, &self.task.id) {
            for child in &tree.children {
                render_dep_node(child, "", true, &mut lines);
            }
        }

        lines
    }
}

fn render_dep_node<'a>(node: &DepNode<'a>, prefix: &str, last: bool, lines: &mut Vec<Line<'a>>) {
    let connector = if last { "└─ " } else { "├─ " };

    let status_color = match node.task.status {
        Status::Done => Color::Green,
        Status::InProgress => Color::Yellow,
        Status::Open => Color::White,
        Status::Blocked => Color::Red,
        Status::Cancelled => Color::DarkGray,
    };

    let mut spans = vec![
        Span::styled(
            format!("{prefix}{connector}"),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            status_indicator(&node.task.status),
            Style::default().fg(status_color),
        ),
        Span::raw(" "),
        Span::styled(
            format!("[{}] ", node.task.id),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw(node.task.title.clone()),
    ];

    if node.cycle {
        spans.push(Span::styled(" (cycle)", Style::default().fg(Color::Red)));
    }

    lines.push(Line::from(spans));

    let child_prefix = format!("{prefix}{}  ", if last { " " } else { "│" });
    let child_count = node.children.len();
    for (i, child) in node.children.iter().enumerate() {
        render_dep_node(child, &child_prefix, i == child_count - 1, lines);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{Priority, Task};

    #[test]
    fn no_deps_returns_empty() {
        let task = Task::new("a".into(), "No deps".into(), Priority::P1);
        let map: HashMap<String, Task> = [("a".into(), task.clone())].into();
        let graph = Graph::build(&map);
        let widget = DepTreeWidget::new(&task, &graph, &map);
        assert!(widget.lines().is_empty());
    }

    #[test]
    fn with_deps_shows_header_and_tree() {
        let mut parent = Task::new("a".into(), "Parent".into(), Priority::P1);
        let child = Task::new("b".into(), "Child".into(), Priority::P1);
        parent.depends_on = vec!["b".into()];

        let map: HashMap<String, Task> = [("a".into(), parent.clone()), ("b".into(), child)].into();
        let graph = Graph::build(&map);
        let widget = DepTreeWidget::new(&parent, &graph, &map);
        let lines = widget.lines();

        assert!(lines.len() >= 3); // blank + header + at least one dep node
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(text.contains("Dependencies"));
        assert!(text.contains("Child"));
    }
}
