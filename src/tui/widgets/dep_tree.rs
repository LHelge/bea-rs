use std::collections::HashMap;

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::graph::{DepNode, Graph};
use crate::task::{Task, TaskType};

use super::super::style::{self, Theme};

pub(in crate::tui) struct DepTreeWidget<'a> {
    task: &'a Task,
    graph: &'a Graph,
    task_map: &'a HashMap<String, Task>,
    theme: &'a Theme,
}

impl<'a> DepTreeWidget<'a> {
    pub fn new(
        task: &'a Task,
        graph: &'a Graph,
        task_map: &'a HashMap<String, Task>,
        theme: &'a Theme,
    ) -> Self {
        Self {
            task,
            graph,
            task_map,
            theme,
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
            self.theme.section_heading_style(),
        )));

        if let Some(tree) = self.graph.dep_tree(self.task_map, &self.task.id) {
            for child in &tree.children {
                render_dep_node(child, "", true, self.theme, &mut lines);
            }
        }

        lines
    }
}

fn render_dep_node<'a>(
    node: &DepNode<'a>,
    prefix: &str,
    last: bool,
    theme: &Theme,
    lines: &mut Vec<Line<'a>>,
) {
    let connector = if last { "└─ " } else { "├─ " };

    let st_color = theme.status_color(&node.task.status);

    let mut spans = vec![
        Span::styled(
            format!("{prefix}{connector}"),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            style::status_indicator(&node.task.status),
            Style::default().fg(st_color),
        ),
        Span::raw(" "),
        Span::styled(
            format!("[{}] ", node.task.id),
            Style::default().fg(theme.id_color),
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
        render_dep_node(child, &child_prefix, i == child_count - 1, theme, lines);
    }
}

/// Shows the dependency graph of all subtasks belonging to an epic.
pub(in crate::tui) struct SubtaskGraphWidget<'a> {
    task: &'a Task,
    graph: &'a Graph,
    task_map: &'a HashMap<String, Task>,
    theme: &'a Theme,
}

impl<'a> SubtaskGraphWidget<'a> {
    pub fn new(
        task: &'a Task,
        graph: &'a Graph,
        task_map: &'a HashMap<String, Task>,
        theme: &'a Theme,
    ) -> Self {
        Self {
            task,
            graph,
            task_map,
            theme,
        }
    }

    pub fn lines(&self) -> Vec<Line<'a>> {
        if self.task.task_type != TaskType::Epic {
            return Vec::new();
        }

        // Collect child tasks (those with parent == this epic's id)
        let mut children: Vec<&Task> = self
            .task_map
            .values()
            .filter(|t| t.parent.as_deref() == Some(&self.task.id))
            .collect();

        if children.is_empty() {
            return Vec::new();
        }

        children.sort_by(|a, b| a.priority.cmp(&b.priority).then(a.created.cmp(&b.created)));

        let mut lines = Vec::new();
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Subtasks",
            self.theme.section_heading_style(),
        )));

        let count = children.len();
        for (i, child) in children.iter().enumerate() {
            let last = i == count - 1;
            if let Some(tree) = self.graph.dep_tree(self.task_map, &child.id) {
                render_dep_node(&tree, "", last, self.theme, &mut lines);
            }
        }

        lines
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
        let theme = Theme::default();
        let widget = DepTreeWidget::new(&task, &graph, &map, &theme);
        assert!(widget.lines().is_empty());
    }

    #[test]
    fn with_deps_shows_header_and_tree() {
        let mut parent = Task::new("a".into(), "Parent".into(), Priority::P1);
        let child = Task::new("b".into(), "Child".into(), Priority::P1);
        parent.depends_on = vec!["b".into()];

        let map: HashMap<String, Task> = [("a".into(), parent.clone()), ("b".into(), child)].into();
        let graph = Graph::build(&map);
        let theme = Theme::default();
        let widget = DepTreeWidget::new(&parent, &graph, &map, &theme);
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
