use std::collections::HashMap;
use std::path::PathBuf;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::widgets::{ListState, Widget};

use crate::graph::Graph;
use crate::task::{Status, Task, TaskType};

use super::style::Theme;
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

/// List filtering mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ListMode {
    /// Show only open/in-progress/blocked tasks (default).
    #[default]
    Open,
    /// Show only tasks whose dependencies are all done (ready to work on).
    Ready,
    /// Show only epics.
    Epics,
    /// Show only done/cancelled tasks.
    Archive,
    /// Show everything.
    All,
}

impl ListMode {
    /// Cycle to the next mode.
    pub fn next(self) -> Self {
        match self {
            Self::Open => Self::Ready,
            Self::Ready => Self::Epics,
            Self::Epics => Self::Archive,
            Self::Archive => Self::All,
            Self::All => Self::Open,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Ready => "Ready",
            Self::Epics => "Epics",
            Self::Archive => "Archive",
            Self::All => "All",
        }
    }
}

/// Active filters on the task list.
#[derive(Debug, Clone, Default)]
pub struct Filter {
    pub query: String,
    pub list_mode: ListMode,
}

impl Filter {
    /// Check if a task matches the current filter (excluding Ready logic which needs the graph).
    pub(super) fn matches(&self, task: &Task, query_lower: &str) -> bool {
        match self.list_mode {
            ListMode::Open => {
                if matches!(task.status, Status::Done | Status::Cancelled) {
                    return false;
                }
            }
            ListMode::Archive => {
                if !matches!(task.status, Status::Done | Status::Cancelled) {
                    return false;
                }
            }
            ListMode::Ready => {
                // Ready pre-filters to open tasks only; graph check is done in apply_filter
                if task.status != Status::Open || task.task_type != TaskType::Task {
                    return false;
                }
            }
            ListMode::Epics => {
                if task.task_type != TaskType::Epic {
                    return false;
                }
            }
            ListMode::All => {}
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
    pub(super) theme: Theme,
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
            theme: Theme::default(),
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
            .filter(|t| {
                // For Ready mode, additionally check that all deps are done
                if self.filter.list_mode == ListMode::Ready {
                    t.depends_on.iter().all(|dep_id| {
                        self.task_map
                            .get(dep_id)
                            .is_some_and(|dep| dep.status == Status::Done)
                    })
                } else {
                    true
                }
            })
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
    fn pane_border_style(&self, pane: FocusPane) -> ratatui::style::Style {
        self.theme.border_style(self.focus == pane)
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
                &self.theme,
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
            &self.theme,
        )
        .render(body[1], frame.buffer_mut());

        self.detail_content_height = metrics.content_height;
        self.detail_visible_height = metrics.visible_height;
        self.detail_scroll = self.detail_scroll.min(self.detail_max_scroll());

        // Bottom bar
        BottomBarWidget::new(
            &self.mode,
            &self.focus,
            self.error_message.as_deref(),
            &self.theme,
        )
        .render(outer[1], frame.buffer_mut());

        // Modal overlays
        match &self.mode {
            Mode::StatusSelect { selected, .. } => {
                StatusModalWidget::new(*selected, &self.theme).render(area, frame.buffer_mut());
            }
            Mode::CreateInput { input } => {
                InputModalWidget::new(" New task title ", input, &self.theme)
                    .render(area, frame.buffer_mut());
            }
            Mode::FilterInput { input } => {
                InputPromptWidget::new(" Filter tasks ", input, &self.theme)
                    .render(area, frame.buffer_mut());
            }
            Mode::ConfirmDelete { title, .. } => {
                InputModalWidget::new(&format!(" Delete \"{title}\"? (y/N) "), "", &self.theme)
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
        assert_eq!(app.filter.list_mode, ListMode::Open);
        let has_done = app.tasks.iter().any(|t| t.status == Status::Done);
        assert!(!has_done);
    }
}
