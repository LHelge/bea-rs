use std::collections::{HashMap, HashSet};

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::task::{Task, TaskType};

use super::super::style::{self, Theme};

/// Render a single task as a line, prefixed by `prefix` + `connector`.
fn task_line(prefix: &str, connector: &str, task: &Task, theme: &Theme) -> Line<'static> {
    let st_color = theme.status_color(&task.status);
    Line::from(vec![
        Span::styled(
            format!("{prefix}{connector}"),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            style::status_indicator(&task.status),
            Style::default().fg(st_color),
        ),
        Span::raw(" "),
        Span::styled(
            format!("[{}] ", task.id),
            Style::default().fg(theme.id_color),
        ),
        Span::raw(task.title.clone()),
    ])
}

/// A line for a dependency id that can't be resolved in the active task map
/// (e.g. it has been archived or deleted).
fn unavailable_line(prefix: &str, id: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!("{prefix}[{id}] (unavailable)"),
        Style::default().fg(Color::DarkGray),
    ))
}

/// Lists a task's **direct** dependencies, one line each.
///
/// Dependencies are intentionally NOT expanded recursively: each task appears
/// at most once and the body stays reachable regardless of graph depth. To
/// explore a dependency's own dependencies, open that task.
pub(in crate::tui) struct DepTreeWidget<'a> {
    task: &'a Task,
    task_map: &'a HashMap<String, Task>,
    theme: &'a Theme,
}

impl<'a> DepTreeWidget<'a> {
    pub fn new(task: &'a Task, task_map: &'a HashMap<String, Task>, theme: &'a Theme) -> Self {
        Self {
            task,
            task_map,
            theme,
        }
    }

    pub fn lines(&self) -> Vec<Line<'static>> {
        if self.task.depends_on.is_empty() {
            return Vec::new();
        }

        let mut lines = Vec::new();
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Dependencies",
            self.theme.section_heading_style(),
        )));

        for dep_id in &self.task.depends_on {
            match self.task_map.get(dep_id) {
                Some(dep) => lines.push(task_line("  ", "", dep, self.theme)),
                None => lines.push(unavailable_line("  ", dep_id)),
            }
        }

        lines
    }
}

/// Shows an epic's subtasks as a parent → child tree, recursing into nested
/// epics. Because each task has at most one parent, every task appears once.
pub(in crate::tui) struct SubtaskGraphWidget<'a> {
    task: &'a Task,
    task_map: &'a HashMap<String, Task>,
    theme: &'a Theme,
}

impl<'a> SubtaskGraphWidget<'a> {
    pub fn new(task: &'a Task, task_map: &'a HashMap<String, Task>, theme: &'a Theme) -> Self {
        Self {
            task,
            task_map,
            theme,
        }
    }

    pub fn lines(&self) -> Vec<Line<'static>> {
        if self.task.task_type != TaskType::Epic {
            return Vec::new();
        }

        let mut body = Vec::new();
        // `seen` guards against pathological parent cycles (parent edges aren't
        // cycle-checked) so rendering always terminates.
        let mut seen: HashSet<&str> = HashSet::new();
        seen.insert(self.task.id.as_str());
        render_children(
            &self.task.id,
            self.task_map,
            "",
            self.theme,
            &mut seen,
            &mut body,
        );

        if body.is_empty() {
            return Vec::new();
        }

        let mut lines = Vec::with_capacity(body.len() + 2);
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Subtasks",
            self.theme.section_heading_style(),
        )));
        lines.extend(body);
        lines
    }
}

/// Recursively render the children of `parent_id` (sorted by priority then
/// creation date), expanding nested epics.
fn render_children<'a>(
    parent_id: &str,
    task_map: &'a HashMap<String, Task>,
    prefix: &str,
    theme: &Theme,
    seen: &mut HashSet<&'a str>,
    lines: &mut Vec<Line<'static>>,
) {
    let mut children: Vec<&Task> = task_map
        .values()
        .filter(|t| t.parent.as_deref() == Some(parent_id))
        .collect();
    children.sort_by(|a, b| a.priority.cmp(&b.priority).then(a.created.cmp(&b.created)));

    let count = children.len();
    for (i, child) in children.iter().copied().enumerate() {
        if !seen.insert(child.id.as_str()) {
            continue; // already rendered (corrupt parent cycle) — skip
        }
        let last = i == count - 1;
        let connector = if last { "└─ " } else { "├─ " };
        lines.push(task_line(prefix, connector, child, theme));

        if child.task_type == TaskType::Epic {
            let child_prefix = format!("{prefix}{}  ", if last { " " } else { "│" });
            render_children(&child.id, task_map, &child_prefix, theme, seen, lines);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{Priority, Status};

    fn task(id: &str, title: &str) -> Task {
        Task::new(id.into(), title.into(), Priority::P1)
    }

    fn text_of(lines: &[Line<'_>]) -> String {
        lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect()
    }

    #[test]
    fn no_deps_returns_empty() {
        let t = task("a", "No deps");
        let map: HashMap<String, Task> = [("a".into(), t.clone())].into();
        assert!(
            DepTreeWidget::new(&t, &map, &Theme::default())
                .lines()
                .is_empty()
        );
    }

    #[test]
    fn shows_only_direct_dependencies() {
        // root → a, b ; a → shared ; b → shared.
        // The Dependencies section must show ONLY the direct deps (a, b), never
        // the transitive `shared` — and therefore never the same task twice.
        let mut root = task("root", "Root");
        root.depends_on = vec!["a".into(), "b".into()];
        let mut a = task("a", "Alpha");
        a.depends_on = vec!["shared".into()];
        let mut b = task("b", "Bravo");
        b.depends_on = vec!["shared".into()];
        let shared = task("shared", "Shared");

        let map: HashMap<String, Task> = [
            ("root".into(), root.clone()),
            ("a".into(), a),
            ("b".into(), b),
            ("shared".into(), shared),
        ]
        .into();

        let text = text_of(&DepTreeWidget::new(&root, &map, &Theme::default()).lines());

        assert!(text.contains("Dependencies"));
        assert!(text.contains("[a]") && text.contains("[b]"));
        assert!(
            !text.contains("[shared]"),
            "transitive dep must not be shown: {text}"
        );
    }

    #[test]
    fn unavailable_dependency_is_marked() {
        let mut t = task("a", "Has missing dep");
        t.depends_on = vec!["gone".into()];
        let map: HashMap<String, Task> = [("a".into(), t.clone())].into();
        let text = text_of(&DepTreeWidget::new(&t, &map, &Theme::default()).lines());
        assert!(text.contains("[gone]") && text.contains("unavailable"));
    }

    #[test]
    fn subtasks_render_only_for_epics() {
        let t = task("a", "Plain task");
        let map: HashMap<String, Task> = [("a".into(), t.clone())].into();
        assert!(
            SubtaskGraphWidget::new(&t, &map, &Theme::default())
                .lines()
                .is_empty()
        );
    }

    #[test]
    fn subtask_tree_expands_nested_epics_each_once() {
        // epic e → c1, e2(epic) ; e2 → g
        let mut e = task("e", "Epic");
        e.task_type = TaskType::Epic;
        let mut c1 = task("c1", "Child one");
        c1.parent = Some("e".into());
        let mut e2 = task("e2", "Inner epic");
        e2.task_type = TaskType::Epic;
        e2.parent = Some("e".into());
        let mut g = task("g", "Grandchild");
        g.status = Status::Done;
        g.parent = Some("e2".into());

        let map: HashMap<String, Task> = [
            ("e".into(), e.clone()),
            ("c1".into(), c1),
            ("e2".into(), e2),
            ("g".into(), g),
        ]
        .into();

        let lines = SubtaskGraphWidget::new(&e, &map, &Theme::default()).lines();
        let text = text_of(&lines);

        assert!(text.contains("Subtasks"));
        // Every descendant appears exactly once.
        for id in ["[c1]", "[e2]", "[g]"] {
            assert_eq!(
                text.matches(id).count(),
                1,
                "{id} should appear once: {text}"
            );
        }
    }
}
