use std::collections::HashMap;

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::graph::{DepNode, Graph};
use crate::task::{Task, TaskType};

use super::super::style::{self, Theme};

/// Maximum depth (number of tree levels) rendered for dependencies and subtasks.
///
/// Nodes below this depth are replaced by a compact `↳ N more…` summary line
/// so that the task body remains reachable without excessive scrolling even on
/// heavily-coupled graphs.
const MAX_DEPTH: usize = 3;

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
                render_dep_node(child, "", true, 0, self.theme, &mut lines);
            }
        }

        lines
    }
}

/// Render one [`DepNode`] and its children into `lines`.
///
/// `depth` is 0 for direct dependencies of the viewed task.  When `depth`
/// reaches [`MAX_DEPTH`] the node is still rendered (as a one-liner) but its
/// children are replaced by a compact "↳ N more…" truncation marker instead
/// of being expanded further, keeping the total line count proportional to
/// the depth cap rather than the graph size.
fn render_dep_node<'a>(
    node: &DepNode<'a>,
    prefix: &str,
    last: bool,
    depth: usize,
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
    } else if node.seen {
        spans.push(Span::styled(
            " (see above)",
            Style::default().fg(Color::DarkGray),
        ));
    }

    lines.push(Line::from(spans));

    // Stop recursing when the depth cap is reached or the node is a leaf (cycle/seen/no children).
    if node.cycle || node.seen || node.children.is_empty() {
        return;
    }

    let child_prefix = format!("{prefix}{}  ", if last { " " } else { "│" });

    if depth >= MAX_DEPTH {
        // Depth cap reached: show a compact truncation marker instead of expanding further.
        let hidden = node.children.len();
        let marker_line = Line::from(vec![Span::styled(
            format!("{child_prefix}↳ {hidden} more…"),
            Style::default().fg(Color::DarkGray),
        )]);
        lines.push(marker_line);
        return;
    }

    let child_count = node.children.len();
    for (i, child) in node.children.iter().enumerate() {
        render_dep_node(
            child,
            &child_prefix,
            i == child_count - 1,
            depth + 1,
            theme,
            lines,
        );
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
                render_dep_node(&tree, "", last, 0, self.theme, &mut lines);
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

    /// A deep diamond graph must render in a bounded number of lines (linear,
    /// not exponential) thanks to the `seen` deduplication in `dep_tree` and
    /// the `MAX_DEPTH` cap in `render_dep_node`.
    ///
    /// Graph:
    ///   root → l1a, l1b
    ///   l1a  → l2a, l2b
    ///   l1b  → l2a, l2b   (shared — second visit is `seen`)
    ///   l2a  → leaf
    ///   l2b  → leaf        (shared — second visit is `seen`)
    ///   leaf → (none)
    ///
    /// Without caps this would be exponential; with caps the line count is
    /// bounded by MAX_DEPTH levels × branching factor + header overhead.
    #[test]
    fn dep_tree_line_count_bounded_for_deep_diamond() {
        let mut root = Task::new("root".into(), "Root".into(), Priority::P1);
        let mut l1a = Task::new("l1a".into(), "L1A".into(), Priority::P1);
        let mut l1b = Task::new("l1b".into(), "L1B".into(), Priority::P1);
        let mut l2a = Task::new("l2a".into(), "L2A".into(), Priority::P1);
        let mut l2b = Task::new("l2b".into(), "L2B".into(), Priority::P1);
        let leaf = Task::new("leaf".into(), "Leaf".into(), Priority::P1);

        root.depends_on = vec!["l1a".into(), "l1b".into()];
        l1a.depends_on = vec!["l2a".into(), "l2b".into()];
        l1b.depends_on = vec!["l2a".into(), "l2b".into()];
        l2a.depends_on = vec!["leaf".into()];
        l2b.depends_on = vec!["leaf".into()];

        let map: HashMap<String, Task> = [
            ("root".into(), root.clone()),
            ("l1a".into(), l1a),
            ("l1b".into(), l1b),
            ("l2a".into(), l2a),
            ("l2b".into(), l2b),
            ("leaf".into(), leaf),
        ]
        .into();
        let graph = Graph::build(&map);
        let theme = Theme::default();
        let widget = DepTreeWidget::new(&root, &graph, &map, &theme);
        let lines = widget.lines();

        // The line count must be bounded. MAX_DEPTH=3 means we expand at most
        // 3 levels (depth 0, 1, 2) before truncating; with a branching factor
        // of 2 that is at most 2+4+8 = 14 node lines + 2 header lines = 16.
        // We use a generous upper bound of 30 to allow for marker lines.
        let line_count = lines.len();
        assert!(
            line_count <= 30,
            "dep tree rendered {line_count} lines for a 6-node diamond — expected ≤ 30"
        );

        // Must still contain the header.
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(text.contains("Dependencies"), "header must be present");
    }

    /// Seen nodes must render as one line (no children expanded below them).
    #[test]
    fn seen_node_renders_as_single_line_with_marker() {
        // Diamond: root → a, b; a → shared; b → shared
        let mut root = Task::new("root".into(), "Root".into(), Priority::P1);
        let mut a = Task::new("a".into(), "A".into(), Priority::P1);
        let mut b = Task::new("b".into(), "B".into(), Priority::P1);
        let shared = Task::new("shared".into(), "Shared".into(), Priority::P1);

        root.depends_on = vec!["a".into(), "b".into()];
        a.depends_on = vec!["shared".into()];
        b.depends_on = vec!["shared".into()];

        let map: HashMap<String, Task> = [
            ("root".into(), root.clone()),
            ("a".into(), a),
            ("b".into(), b),
            ("shared".into(), shared),
        ]
        .into();
        let graph = Graph::build(&map);
        let theme = Theme::default();
        let widget = DepTreeWidget::new(&root, &graph, &map, &theme);
        let lines = widget.lines();

        // Count occurrences of "shared" in the rendered output.
        // It must appear exactly twice: once as a full node, once as "(see above)".
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();

        let shared_count = text.matches("Shared").count();
        assert_eq!(
            shared_count, 2,
            "shared node should appear exactly twice: once fully, once as (see above)"
        );
        assert!(
            text.contains("see above"),
            "second occurrence of shared node must be marked as (see above)"
        );
    }
}
