use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::widgets::{ListState, Widget};

use crate::graph;
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
    /// Show done/cancelled tasks still in the active store (not yet archived).
    Completed,
    /// Show tasks that have been archived to `.bears/archive/`.
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
            Self::Epics => Self::Completed,
            Self::Completed => Self::Archive,
            Self::Archive => Self::All,
            Self::All => Self::Open,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Ready => "Ready",
            Self::Epics => "Epics",
            Self::Completed => "Completed",
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
            ListMode::Completed => {
                if !matches!(task.status, Status::Done | Status::Cancelled) {
                    return false;
                }
            }
            // Archive draws from the on-disk archive set (which is already all
            // settled tasks); only the search query applies here.
            ListMode::Archive => {}
            ListMode::Ready => {
                // Quick pre-filter before the full readiness check (graph::is_task_ready)
                // that is applied in apply_filter.
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
    /// Tasks loaded from `.bears/archive/` (shown only in `ListMode::Archive`).
    pub archived_tasks: Vec<Task>,
    pub tasks: Vec<Task>,
    pub task_map: HashMap<String, Task>,
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
            archived_tasks: Vec::new(),
            tasks: filtered,
            task_map,
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

    /// Task lookup for the detail pane: the active task map, augmented with
    /// archived tasks so an archived task's dependencies and subtasks (which
    /// also live in the archive) still resolve. Borrows the active map directly
    /// when there is no archive to merge in.
    fn detail_task_map(&self) -> Cow<'_, HashMap<String, Task>> {
        if self.archived_tasks.is_empty() {
            Cow::Borrowed(&self.task_map)
        } else {
            let mut map = self.task_map.clone();
            for t in &self.archived_tasks {
                map.entry(t.id.clone()).or_insert_with(|| t.clone());
            }
            Cow::Owned(map)
        }
    }

    /// Reload task data (called after disk changes).
    ///
    /// Preserves:
    /// - The selected task **by id** (follows if it moved in the list).
    /// - Falls back to the nearest neighbour (old index clamped) when the task
    ///   has been deleted, or to the first task when the list is now empty.
    /// - The current `filter.list_mode` and `filter.query` (re-applies them).
    /// - The detail-pane scroll offset, clamped to the new content height so it
    ///   never points past the end after the content shrinks.
    pub fn reload(&mut self, tasks: Vec<Task>, task_map: HashMap<String, Task>) {
        let old_id = self.selected_task().map(|t| t.id.clone());
        let old_idx = self.list_state.selected();

        self.all_tasks = tasks;
        self.task_map = task_map;
        self.apply_filter_with_fallback(old_id.as_deref(), old_idx);

        // Clamp detail scroll to the new content height. The exact content height
        // is only known after rendering, but we can clamp conservatively here so
        // the scroll is never obviously wrong. It will also be re-clamped in
        // render() once the new metrics are known.
        self.detail_scroll = self.detail_scroll.min(self.detail_max_scroll());
    }

    /// Reload both the active task set and the archived task set together.
    /// Used by the live watcher so archive/restore is reflected immediately.
    pub(super) fn reload_with_archived(
        &mut self,
        tasks: Vec<Task>,
        task_map: HashMap<String, Task>,
        archived: Vec<Task>,
    ) {
        // Set archived first so a reload while viewing the Archive uses the
        // fresh set when the filter re-applies.
        self.archived_tasks = archived;
        self.reload(tasks, task_map);
    }

    /// Recompute the filtered task list from all_tasks + current filter.
    pub(super) fn apply_filter(&mut self) {
        let selected_id = self.selected_task().map(|t| t.id.clone());
        let old_idx = self.list_state.selected();
        self.apply_filter_with_fallback(selected_id.as_deref(), old_idx);
    }

    /// Internal: recompute filtered list, then restore selection.
    ///
    /// Priority:
    /// 1. Find `old_id` in the new list → use its new position.
    /// 2. Clamp `old_idx` to the new list length (nearest neighbour).
    /// 3. Select the first task if the list is non-empty.
    /// 4. Select nothing if the list is empty.
    fn apply_filter_with_fallback(&mut self, old_id: Option<&str>, old_idx: Option<usize>) {
        let query_lower = self.filter.query.to_lowercase();
        // Archive mode lists tasks from the on-disk archive (`.bears/archive/`);
        // every other mode filters the active task set.
        let source = if self.filter.list_mode == ListMode::Archive {
            &self.archived_tasks
        } else {
            &self.all_tasks
        };
        self.tasks = source
            .iter()
            .filter(|t| self.filter.matches(t, &query_lower))
            .filter(|t| {
                // For Ready mode, delegate to the canonical graph predicate so the
                // rule lives in exactly one place.
                self.filter.list_mode != ListMode::Ready || graph::is_task_ready(&self.task_map, t)
            })
            .cloned()
            .collect();

        let new_idx = if self.tasks.is_empty() {
            None
        } else if let Some(id) = old_id
            && let Some(pos) = self.tasks.iter().position(|t| t.id == id)
        {
            // Task still exists: follow it by id.
            Some(pos)
        } else if let Some(old) = old_idx {
            // Task disappeared: fall back to nearest neighbour (clamped).
            Some(old.min(self.tasks.len() - 1))
        } else {
            // No prior selection: pick the first item.
            Some(0)
        };

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

        // Detail view. An archived task's dependencies and subtasks live in the
        // archive, not the active store, so resolve the detail pane against
        // active ∪ archived.
        let mut metrics = DetailMetrics {
            content_height: 0,
            visible_height: 0,
        };
        let detail_map = self.detail_task_map();
        TaskDetailWidget::new(
            self.selected_task(),
            &detail_map,
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
    use crate::task::{Priority, Status};

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

    #[test]
    fn test_completed_and_archive_modes_use_different_sources() {
        // Active store has aaa(Open), bbb(InProgress), ccc(Done).
        let mut app = make_app();
        // The archived set lives in .bears/archive/ — not in `all_tasks`.
        let mut archived = Task::new("zzz".into(), "Archived task".into(), Priority::P2);
        archived.status = Status::Done;
        app.archived_tasks = vec![archived];

        // Completed = done/cancelled tasks still in the ACTIVE store.
        app.filter.list_mode = ListMode::Completed;
        app.apply_filter();
        assert_eq!(
            app.tasks.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            vec!["ccc"],
            "Completed should show active done/cancelled tasks"
        );

        // Archive = the on-disk archived set, independent of the active store.
        app.filter.list_mode = ListMode::Archive;
        app.apply_filter();
        assert_eq!(
            app.tasks.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            vec!["zzz"],
            "Archive should show the archived set, not the active store"
        );
    }

    #[test]
    fn detail_task_map_includes_archived() {
        // Active store empty; an archived epic with an archived child — exactly
        // the all-archived case where the subtask tree was rendering empty.
        let mut app = App::new(vec![], HashMap::new(), std::path::PathBuf::from("."));
        let mut epic = Task::new("ep".into(), "Epic".into(), Priority::P1);
        epic.task_type = TaskType::Epic;
        let mut child = Task::new("ch".into(), "Child".into(), Priority::P2);
        child.parent = Some("ep".into());
        child.status = Status::Done;
        app.archived_tasks = vec![epic, child];

        let map = app.detail_task_map();
        assert!(
            map.contains_key("ep") && map.contains_key("ch"),
            "detail map must include archived tasks so an archived epic resolves its children"
        );
    }

    /// The TUI Ready filter must agree with `graph::is_task_ready` on every task.
    /// Specifically: a task with an unsatisfied dep must be excluded, and one
    /// whose deps are all Done must be included.
    #[test]
    fn test_ready_filter_agrees_with_is_task_ready() {
        use crate::graph;
        use crate::task::{Priority, Task};

        // Build a small set of tasks with varying readiness:
        //  - "ready":    Open, dep on "done_dep" (Done)  → ready
        //  - "blocked":  Open, dep on "open_dep" (Open)  → NOT ready
        //  - "missing":  Open, dep on "ghost" (absent)   → NOT ready
        //  - "done_dep": Done, no deps                   → not even Open
        //  - "open_dep": Open, no deps                   → ready itself, but blocks "blocked"
        let mut tasks_vec = Vec::new();
        let make = |id: &str, status: Status, deps: Vec<&str>| {
            let mut t = Task::new(id.to_string(), format!("Task {id}"), Priority::P1);
            t.status = status;
            t.depends_on = deps.into_iter().map(String::from).collect();
            t
        };
        tasks_vec.push(make("ready", Status::Open, vec!["done_dep"]));
        tasks_vec.push(make("blocked", Status::Open, vec!["open_dep"]));
        tasks_vec.push(make("missing", Status::Open, vec!["ghost"]));
        tasks_vec.push(make("done_dep", Status::Done, vec![]));
        tasks_vec.push(make("open_dep", Status::Open, vec![]));

        let task_map: HashMap<String, Task> = tasks_vec
            .iter()
            .map(|t| (t.id.clone(), t.clone()))
            .collect();

        let mut app = App::new(tasks_vec, task_map.clone(), std::path::PathBuf::from("."));
        app.filter.list_mode = ListMode::Ready;
        app.apply_filter();

        let tui_ready_ids: std::collections::HashSet<&str> =
            app.tasks.iter().map(|t| t.id.as_str()).collect();

        for task in task_map.values() {
            let predicate = graph::is_task_ready(&task_map, task);
            let in_tui = tui_ready_ids.contains(task.id.as_str());
            assert_eq!(
                predicate, in_tui,
                "TUI Ready filter and graph::is_task_ready disagree on task '{}'",
                task.id
            );
        }

        // Explicit spot-checks for clarity
        assert!(
            tui_ready_ids.contains("ready"),
            "'ready' task should appear in TUI Ready list"
        );
        assert!(
            !tui_ready_ids.contains("blocked"),
            "'blocked' task (unsatisfied dep) must NOT appear in TUI Ready list"
        );
        assert!(
            !tui_ready_ids.contains("missing"),
            "'missing' task (absent dep) must NOT appear in TUI Ready list"
        );
    }
}
