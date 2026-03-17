use std::collections::HashMap;
use std::path::PathBuf;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::widgets::{ListState, Widget};

use crate::graph::Graph;
use crate::task::{Status, Task};

use super::widgets::{
    BottomBarWidget, DetailMetrics, InputModalWidget, InputPromptWidget, StatusModalWidget,
    TaskDetailWidget, TaskListWidget,
};

/// Action returned by handle_key for the event loop to process.
#[derive(Debug)]
pub enum Action {
    None,
    Quit,
    EditSelected,
    CreateTask(String),
    UpdateStatus(String, Status),
    DeleteTask(String),
}

/// Which pane is focused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusPane {
    List,
    Detail,
}

/// Current interaction mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Normal,
    StatusSelect { task_id: String, selected: usize },
    CreateInput { input: String },
    FilterInput { input: String },
    ConfirmDelete { task_id: String, title: String },
}

/// Active filters on the task list.
#[derive(Debug, Clone, Default)]
pub struct Filter {
    pub query: String,
    pub show_all: bool,
}

impl Filter {
    /// Check if a task matches the current filter.
    pub(super) fn matches(&self, task: &Task, query_lower: &str) -> bool {
        if !self.show_all && matches!(task.status, Status::Done | Status::Cancelled) {
            return false;
        }
        if !query_lower.is_empty() {
            let in_title = task.title.to_lowercase().contains(query_lower);
            let in_id = task.id.to_lowercase().contains(query_lower);
            let in_tags = task
                .tags
                .iter()
                .any(|t| t.to_lowercase().contains(query_lower));
            if !in_title && !in_id && !in_tags {
                return false;
            }
        }
        true
    }

    pub(super) fn is_active(&self) -> bool {
        !self.query.is_empty() || self.show_all
    }
}

/// Core TUI application state.
pub struct App {
    pub all_tasks: Vec<Task>,
    pub tasks: Vec<Task>,
    pub task_map: HashMap<String, Task>,
    pub graph: Graph,
    pub list_state: ListState,
    pub base: PathBuf,
    pub mode: Mode,
    pub filter: Filter,
    pub error_message: Option<String>,
    pub focus: FocusPane,
    pub detail_scroll: u16,
    pub(super) detail_content_height: u16,
    pub(super) detail_visible_height: u16,
    pub(super) last_selected_id: Option<String>,
}

impl App {
    pub fn new(tasks: Vec<Task>, task_map: HashMap<String, Task>, base: PathBuf) -> Self {
        let graph = Graph::build(&task_map);
        let filter = Filter::default();
        let query_lower = filter.query.to_lowercase();
        let filtered: Vec<Task> = tasks
            .iter()
            .filter(|t| filter.matches(t, &query_lower))
            .cloned()
            .collect();
        let mut list_state = ListState::default();
        if !filtered.is_empty() {
            list_state.select(Some(0));
        }
        let last_selected_id = filtered.first().map(|t| t.id.clone());
        Self {
            all_tasks: tasks,
            tasks: filtered,
            task_map,
            graph,
            list_state,
            base,
            mode: Mode::Normal,
            filter,
            error_message: None,
            focus: FocusPane::List,
            detail_scroll: 0,
            detail_content_height: 0,
            detail_visible_height: 0,
            last_selected_id,
        }
    }

    pub fn selected_index(&self) -> Option<usize> {
        self.list_state.selected()
    }

    pub fn selected_task(&self) -> Option<&Task> {
        self.selected_index().and_then(|i| self.tasks.get(i))
    }

    /// Reload task data (called after disk changes).
    pub fn reload(&mut self, tasks: Vec<Task>, task_map: HashMap<String, Task>) {
        self.graph = Graph::build(&task_map);
        self.all_tasks = tasks;
        self.task_map = task_map;
        self.apply_filter();
    }

    /// Recompute the filtered task list from all_tasks + current filter.
    pub(super) fn apply_filter(&mut self) {
        let selected_id = self.selected_task().map(|t| t.id.clone());
        let query_lower = self.filter.query.to_lowercase();
        self.tasks = self
            .all_tasks
            .iter()
            .filter(|t| self.filter.matches(t, &query_lower))
            .cloned()
            .collect();
        let new_idx = selected_id
            .and_then(|id| self.tasks.iter().position(|t| t.id == id))
            .or(if self.tasks.is_empty() { None } else { Some(0) });
        self.list_state.select(new_idx);
    }

    /// Reset detail scroll when the selected task changes.
    pub(super) fn check_scroll_reset(&mut self) {
        let current_id = self.selected_task().map(|t| t.id.clone());
        if current_id != self.last_selected_id {
            self.detail_scroll = 0;
            self.last_selected_id = current_id;
        }
    }

    /// Maximum scroll offset for the detail pane.
    pub(super) fn detail_max_scroll(&self) -> u16 {
        self.detail_content_height
            .saturating_sub(self.detail_visible_height)
    }

    /// Border style depending on whether a pane is focused.
    fn pane_border_style(&self, pane: FocusPane) -> Style {
        if self.focus == pane {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        }
    }

    /// Render the full UI.
    pub fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();

        // Main layout: body + bottom bar
        let outer = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(area);

        // Body: left panel (task list) + right panel (detail)
        let list_width = (outer[0].width * 40 / 100).min(40);
        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(list_width), Constraint::Min(1)])
            .split(outer[0]);

        // Task list
        frame.render_stateful_widget(
            TaskListWidget::new(
                &self.tasks,
                &self.filter,
                self.pane_border_style(FocusPane::List),
            ),
            body[0],
            &mut self.list_state,
        );

        // Detail view
        let mut metrics = DetailMetrics {
            content_height: 0,
            visible_height: 0,
        };
        TaskDetailWidget::new(
            self.selected_task(),
            &self.graph,
            &self.task_map,
            self.detail_scroll,
            self.pane_border_style(FocusPane::Detail),
            &mut metrics,
        )
        .render(body[1], frame.buffer_mut());

        self.detail_content_height = metrics.content_height;
        self.detail_visible_height = metrics.visible_height;
        self.detail_scroll = self.detail_scroll.min(self.detail_max_scroll());

        // Bottom bar
        BottomBarWidget::new(
            &self.mode,
            &self.focus,
            &self.filter,
            self.error_message.as_deref(),
        )
        .render(outer[1], frame.buffer_mut());

        // Modal overlays
        match &self.mode {
            Mode::StatusSelect { selected, .. } => {
                (StatusModalWidget {
                    selected: *selected,
                })
                .render(area, frame.buffer_mut());
            }
            Mode::CreateInput { input } => {
                (InputModalWidget {
                    title: " New task title ",
                    input,
                })
                .render(area, frame.buffer_mut());
            }
            Mode::FilterInput { input } => {
                (InputPromptWidget {
                    title: " Filter tasks ",
                    input,
                })
                .render(area, frame.buffer_mut());
            }
            Mode::ConfirmDelete { title, .. } => {
                (InputModalWidget {
                    title: &format!(" Delete \"{title}\"? (y/N) "),
                    input: "",
                })
                .render(area, frame.buffer_mut());
            }
            Mode::Normal => {}
        }
    }
}

#[cfg(test)]
pub(super) mod test_helpers {
    use super::*;
    use crate::task::{Priority, Task};
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    pub fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    pub fn sample_tasks() -> (Vec<Task>, HashMap<String, Task>) {
        let mut tasks = Vec::new();
        for (id, title, status) in [
            ("aaa", "Task A", Status::Open),
            ("bbb", "Task B", Status::InProgress),
            ("ccc", "Task C", Status::Done),
        ] {
            let mut t = Task::new(id.to_string(), title.to_string(), Priority::P1);
            t.status = status;
            tasks.push(t);
        }
        let map: HashMap<String, Task> = tasks.iter().map(|t| (t.id.clone(), t.clone())).collect();
        (tasks, map)
    }

    pub fn make_app() -> App {
        let (tasks, map) = sample_tasks();
        App::new(tasks, map, std::path::PathBuf::from("."))
    }
}

#[cfg(test)]
mod tests {
    use super::test_helpers::*;
    use super::*;
    use crate::task::Status;

    #[test]
    fn test_empty_app() {
        let app = App::new(vec![], HashMap::new(), std::path::PathBuf::from("."));
        assert_eq!(app.selected_index(), None);
        assert!(app.selected_task().is_none());
    }

    #[test]
    fn test_filter_default_hides_done() {
        let app = make_app();
        assert!(!app.filter.show_all);
        let has_done = app.tasks.iter().any(|t| t.status == Status::Done);
        assert!(!has_done);
    }
}
