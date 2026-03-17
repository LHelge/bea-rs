use std::collections::HashMap;
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};

use crate::graph::{DepNode, Graph};
use crate::task::{Priority, Status, Task, TaskType};

const ALL_STATUSES: &[Status] = &[
    Status::Open,
    Status::InProgress,
    Status::Done,
    Status::Blocked,
    Status::Cancelled,
];

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
    /// `query_lower` is the pre-lowercased query, computed once per filter application.
    fn matches(&self, task: &Task, query_lower: &str) -> bool {
        // Hide done/cancelled unless show_all
        if !self.show_all && matches!(task.status, Status::Done | Status::Cancelled) {
            return false;
        }
        // Text filter (title, id, tags)
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

    fn is_active(&self) -> bool {
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
    detail_content_height: u16,
    detail_visible_height: u16,
    last_selected_id: Option<String>,
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
    fn apply_filter(&mut self) {
        let selected_id = self.selected_task().map(|t| t.id.clone());
        let query_lower = self.filter.query.to_lowercase();
        self.tasks = self
            .all_tasks
            .iter()
            .filter(|t| self.filter.matches(t, &query_lower))
            .cloned()
            .collect();
        // Try to preserve selection
        let new_idx = selected_id
            .and_then(|id| self.tasks.iter().position(|t| t.id == id))
            .or(if self.tasks.is_empty() { None } else { Some(0) });
        self.list_state.select(new_idx);
    }

    /// Reset detail scroll when the selected task changes.
    fn check_scroll_reset(&mut self) {
        let current_id = self.selected_task().map(|t| t.id.clone());
        if current_id != self.last_selected_id {
            self.detail_scroll = 0;
            self.last_selected_id = current_id;
        }
    }

    /// Handle a key press event; returns an Action for the event loop.
    pub fn handle_key(&mut self, key: KeyEvent) -> Action {
        let action = match &self.mode {
            Mode::Normal => self.handle_normal_key(key),
            Mode::StatusSelect { .. } => self.handle_status_select_key(key),
            Mode::CreateInput { .. } => self.handle_create_input_key(key),
            Mode::FilterInput { .. } => self.handle_filter_input_key(key),
            Mode::ConfirmDelete { .. } => self.handle_confirm_delete_key(key),
        };
        self.check_scroll_reset();
        action
    }

    fn handle_normal_key(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => Action::Quit,
            KeyCode::Tab => {
                self.focus = match self.focus {
                    FocusPane::List => FocusPane::Detail,
                    FocusPane::Detail => FocusPane::List,
                };
                Action::None
            }
            KeyCode::Right if self.focus == FocusPane::List => {
                self.focus = FocusPane::Detail;
                Action::None
            }
            KeyCode::Left if self.focus == FocusPane::Detail => {
                self.focus = FocusPane::List;
                Action::None
            }
            KeyCode::Char('j') | KeyCode::Down => {
                match self.focus {
                    FocusPane::List => self.select_next(),
                    FocusPane::Detail => {
                        self.detail_scroll = self
                            .detail_scroll
                            .saturating_add(1)
                            .min(self.detail_max_scroll());
                    }
                }
                Action::None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                match self.focus {
                    FocusPane::List => self.select_prev(),
                    FocusPane::Detail => self.detail_scroll = self.detail_scroll.saturating_sub(1),
                }
                Action::None
            }
            KeyCode::Home | KeyCode::Char('g') => {
                match self.focus {
                    FocusPane::List => self.select_first(),
                    FocusPane::Detail => self.detail_scroll = 0,
                }
                Action::None
            }
            KeyCode::End | KeyCode::Char('G') => {
                match self.focus {
                    FocusPane::List => self.select_last(),
                    FocusPane::Detail => { /* no max clamp needed, ratatui handles it */ }
                }
                Action::None
            }
            KeyCode::Enter if self.focus == FocusPane::List => {
                self.focus = FocusPane::Detail;
                Action::None
            }
            KeyCode::Char('e') => Action::EditSelected,
            KeyCode::Char('c') => {
                self.mode = Mode::CreateInput {
                    input: String::new(),
                };
                Action::None
            }
            KeyCode::Char('s') => {
                if let Some(task) = self.selected_task() {
                    let current_idx = ALL_STATUSES
                        .iter()
                        .position(|s| *s == task.status)
                        .unwrap_or(0);
                    self.mode = Mode::StatusSelect {
                        task_id: task.id.clone(),
                        selected: current_idx,
                    };
                }
                Action::None
            }
            KeyCode::Char('/') => {
                self.mode = Mode::FilterInput {
                    input: self.filter.query.clone(),
                };
                Action::None
            }
            KeyCode::Char('a') => {
                self.filter.show_all = !self.filter.show_all;
                self.apply_filter();
                Action::None
            }
            KeyCode::Char('d') => {
                if let Some(task) = self.selected_task() {
                    self.mode = Mode::ConfirmDelete {
                        task_id: task.id.clone(),
                        title: task.title.clone(),
                    };
                }
                Action::None
            }
            _ => Action::None,
        }
    }

    fn handle_status_select_key(&mut self, key: KeyEvent) -> Action {
        let (task_id, selected) = match &self.mode {
            Mode::StatusSelect { task_id, selected } => (task_id.clone(), *selected),
            _ => return Action::None,
        };

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.mode = Mode::Normal;
                Action::None
            }
            KeyCode::Char('j') | KeyCode::Down => {
                let next = if selected >= ALL_STATUSES.len() - 1 {
                    0
                } else {
                    selected + 1
                };
                self.mode = Mode::StatusSelect {
                    task_id,
                    selected: next,
                };
                Action::None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                let prev = if selected == 0 {
                    ALL_STATUSES.len() - 1
                } else {
                    selected - 1
                };
                self.mode = Mode::StatusSelect {
                    task_id,
                    selected: prev,
                };
                Action::None
            }
            KeyCode::Enter => {
                let status = ALL_STATUSES[selected].clone();
                self.mode = Mode::Normal;
                Action::UpdateStatus(task_id, status)
            }
            _ => Action::None,
        }
    }

    /// Shared text-input editing: handles Esc, Backspace, Char.
    /// Returns Some(updated_input) for editing keys, or None for Esc (cancel).
    /// Enter is NOT handled here — callers handle it for mode-specific behavior.
    fn handle_text_input_key(key: KeyEvent, input: &str) -> Option<Option<String>> {
        match key.code {
            KeyCode::Esc => Some(None), // cancel
            KeyCode::Backspace => {
                let mut new_input = input.to_string();
                new_input.pop();
                Some(Some(new_input))
            }
            KeyCode::Char(c) => {
                let mut new_input = input.to_string();
                new_input.push(c);
                Some(Some(new_input))
            }
            _ => None, // unhandled
        }
    }

    fn handle_create_input_key(&mut self, key: KeyEvent) -> Action {
        let input = match &self.mode {
            Mode::CreateInput { input } => input.clone(),
            _ => return Action::None,
        };

        if key.code == KeyCode::Enter {
            let title = input.trim().to_string();
            self.mode = Mode::Normal;
            return if title.is_empty() {
                Action::None
            } else {
                Action::CreateTask(title)
            };
        }

        match Self::handle_text_input_key(key, &input) {
            Some(None) => self.mode = Mode::Normal,
            Some(Some(new_input)) => self.mode = Mode::CreateInput { input: new_input },
            None => {}
        }
        Action::None
    }

    fn handle_filter_input_key(&mut self, key: KeyEvent) -> Action {
        let input = match &self.mode {
            Mode::FilterInput { input } => input.clone(),
            _ => return Action::None,
        };

        if key.code == KeyCode::Enter {
            self.filter.query = input;
            self.mode = Mode::Normal;
            self.apply_filter();
            return Action::None;
        }

        match Self::handle_text_input_key(key, &input) {
            Some(None) => self.mode = Mode::Normal,
            Some(Some(new_input)) => self.mode = Mode::FilterInput { input: new_input },
            None => {}
        }
        Action::None
    }

    fn handle_confirm_delete_key(&mut self, key: KeyEvent) -> Action {
        let task_id = match &self.mode {
            Mode::ConfirmDelete { task_id, .. } => task_id.clone(),
            _ => return Action::None,
        };

        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.mode = Mode::Normal;
                Action::DeleteTask(task_id)
            }
            _ => {
                // Any other key cancels
                self.mode = Mode::Normal;
                Action::None
            }
        }
    }

    fn select_next(&mut self) {
        if self.tasks.is_empty() {
            return;
        }
        let i = self.selected_index().unwrap_or(0);
        let next = if i >= self.tasks.len() - 1 { 0 } else { i + 1 };
        self.list_state.select(Some(next));
    }

    fn select_prev(&mut self) {
        if self.tasks.is_empty() {
            return;
        }
        let i = self.selected_index().unwrap_or(0);
        let prev = if i == 0 { self.tasks.len() - 1 } else { i - 1 };
        self.list_state.select(Some(prev));
    }

    fn select_first(&mut self) {
        if !self.tasks.is_empty() {
            self.list_state.select(Some(0));
        }
    }

    fn select_last(&mut self) {
        if !self.tasks.is_empty() {
            self.list_state.select(Some(self.tasks.len() - 1));
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

        self.render_task_list(frame, body[0]);
        self.render_detail(frame, body[1]);
        self.render_bottom_bar(frame, outer[1]);

        // Render modal overlays
        match &self.mode {
            Mode::StatusSelect { selected, .. } => {
                self.render_status_modal(frame, area, *selected);
            }
            Mode::CreateInput { input } => {
                Self::render_input_modal(frame, area, " New task title ", input);
            }
            Mode::FilterInput { input } => {
                Self::render_input_prompt(frame, area, " Filter tasks ", input);
            }
            Mode::ConfirmDelete { title, .. } => {
                Self::render_input_modal(frame, area, &format!(" Delete \"{title}\"? (y/N) "), "");
            }
            Mode::Normal => {}
        }
    }

    /// Border style depending on whether a pane is focused.
    fn pane_border_style(&self, pane: FocusPane) -> Style {
        if self.focus == pane {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        }
    }

    /// Left panel: task list with selection and scrolling.
    fn render_task_list(&mut self, frame: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self
            .tasks
            .iter()
            .map(|task| {
                let priority_color = priority_color(task.priority);

                let status_indicator = status_indicator(&task.status);

                let type_prefix = if task.task_type == TaskType::Epic {
                    "◆ "
                } else {
                    ""
                };

                let line = Line::from(vec![
                    Span::styled(
                        format!("{} ", task.priority),
                        Style::default().fg(priority_color),
                    ),
                    Span::styled(
                        format!("{status_indicator} "),
                        Style::default().fg(Color::White),
                    ),
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
                    .border_style(self.pane_border_style(FocusPane::List)),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");

        frame.render_stateful_widget(list, area, &mut self.list_state);
    }

    /// Right panel: detail view with frontmatter, body, and dependency tree.
    /// Maximum scroll offset: content that overflows beyond the visible area.
    fn detail_max_scroll(&self) -> u16 {
        self.detail_content_height
            .saturating_sub(self.detail_visible_height)
    }

    fn render_detail(&mut self, frame: &mut Frame, area: Rect) {
        let detail_border = self.pane_border_style(FocusPane::Detail);
        let content = match self.selected_task() {
            None => {
                let p = Paragraph::new("No tasks found.").block(
                    Block::default()
                        .title(" Detail ")
                        .borders(Borders::ALL)
                        .border_style(detail_border),
                );
                frame.render_widget(p, area);
                return;
            }
            Some(task) => task,
        };

        let mut lines: Vec<Line> = Vec::new();

        // Title
        lines.push(Line::from(Span::styled(
            &content.title,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));

        // Metadata fields
        let label_style = Style::default().fg(Color::Cyan);

        lines.push(Line::from(vec![
            Span::styled("Status:   ", label_style),
            Span::raw(content.status.to_string()),
        ]));

        lines.push(Line::from(vec![
            Span::styled("Priority: ", label_style),
            Span::styled(
                content.priority.to_string(),
                Style::default().fg(priority_color(content.priority)),
            ),
        ]));

        if content.task_type == TaskType::Epic {
            lines.push(Line::from(vec![
                Span::styled("Type:     ", label_style),
                Span::raw("epic"),
            ]));
        }

        if !content.tags.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("Tags:     ", label_style),
                Span::raw(content.tags.join(", ")),
            ]));
        }

        if !content.assignee.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("Assignee: ", label_style),
                Span::raw(&content.assignee),
            ]));
        }

        if let Some(ref parent) = content.parent {
            lines.push(Line::from(vec![
                Span::styled("Parent:   ", label_style),
                Span::raw(parent),
            ]));
        }

        lines.push(Line::from(vec![
            Span::styled("Created:  ", label_style),
            Span::raw(content.created.format("%Y-%m-%d %H:%M").to_string()),
        ]));

        lines.push(Line::from(vec![
            Span::styled("Updated:  ", label_style),
            Span::raw(content.updated.format("%Y-%m-%d %H:%M").to_string()),
        ]));

        // Dependency tree
        if !content.depends_on.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Dependencies",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )));
            if let Some(tree) = self.graph.dep_tree(&self.task_map, &content.id) {
                for child in &tree.children {
                    render_dep_tree(child, "", true, &mut lines);
                }
            }
        }

        // Body
        if !content.body.trim().is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "───────────────────────────────",
                Style::default().fg(Color::DarkGray),
            )));
            lines.push(Line::from(""));
            for body_line in content.body.lines() {
                lines.push(Line::from(body_line.to_string()));
            }
        }

        let content_height = {
            // Inner width = area minus left and right borders
            let inner_width = area.width.saturating_sub(2) as usize;
            if inner_width == 0 {
                lines.len() as u16
            } else {
                lines
                    .iter()
                    .map(|line| {
                        let w = line.width();
                        if w <= inner_width {
                            1u16
                        } else {
                            (w as u16).div_ceil(inner_width as u16)
                        }
                    })
                    .sum()
            }
        };
        let visible_height = area.height.saturating_sub(2);

        let paragraph = Paragraph::new(lines)
            .block(
                Block::default()
                    .title(" Detail ")
                    .borders(Borders::ALL)
                    .border_style(detail_border),
            )
            .wrap(Wrap { trim: false })
            .scroll((self.detail_scroll, 0));
        frame.render_widget(paragraph, area);

        // Update dimensions after paragraph is consumed (releases borrow on self)
        self.detail_content_height = content_height;
        self.detail_visible_height = visible_height;
        self.detail_scroll = self.detail_scroll.min(self.detail_max_scroll());
    }

    /// Bottom bar: keyboard shortcut hints or error message.
    fn render_bottom_bar(&self, frame: &mut Frame, area: Rect) {
        // Show error message if present
        if let Some(ref msg) = self.error_message {
            let bar = Paragraph::new(Line::from(vec![
                Span::styled(" ERROR ", Style::default().fg(Color::White).bg(Color::Red)),
                Span::styled(format!(" {msg} "), Style::default().fg(Color::Red)),
            ]))
            .style(Style::default().bg(Color::Black));
            frame.render_widget(bar, area);
            return;
        }

        let hints: Vec<(&str, &str)> = match &self.mode {
            Mode::Normal => {
                let nav_hint = match self.focus {
                    FocusPane::List => "navigate",
                    FocusPane::Detail => "scroll",
                };
                vec![
                    ("q", "quit"),
                    ("Tab", "switch pane"),
                    ("j/k", nav_hint),
                    ("e", "edit"),
                    ("c", "create"),
                    ("s", "status"),
                    ("d", "delete"),
                    ("/", "filter"),
                    (
                        "a",
                        if self.filter.show_all {
                            "hide done"
                        } else {
                            "show all"
                        },
                    ),
                ]
            }
            Mode::StatusSelect { .. } => {
                vec![("Esc", "cancel"), ("j/k", "navigate"), ("Enter", "confirm")]
            }
            Mode::CreateInput { .. } => vec![("Esc", "cancel"), ("Enter", "create")],
            Mode::FilterInput { .. } => vec![("Esc", "cancel"), ("Enter", "apply")],
            Mode::ConfirmDelete { .. } => vec![("y", "confirm delete"), ("any", "cancel")],
        };

        // Build hint spans, truncating when terminal width is exceeded
        let max_width = area.width as usize;
        let mut spans: Vec<Span> = Vec::new();
        let mut used_width: usize = 0;

        for (i, (key, desc)) in hints.iter().enumerate() {
            let key_text = format!(" {key} ");
            let desc_text = format!(" {desc} ");
            let sep_width = if i < hints.len() - 1 { 1 } else { 0 };
            let entry_width = key_text.len() + desc_text.len() + sep_width;

            if used_width + entry_width > max_width {
                break;
            }

            spans.push(Span::styled(
                key_text,
                Style::default().fg(Color::Black).bg(Color::White),
            ));
            spans.push(Span::styled(
                desc_text,
                Style::default().fg(Color::DarkGray),
            ));
            if i < hints.len() - 1 {
                spans.push(Span::raw(" "));
            }
            used_width += entry_width;
        }

        let bar = Paragraph::new(Line::from(spans)).style(Style::default().bg(Color::Black));
        frame.render_widget(bar, area);
    }

    /// Render status selection modal centered on screen.
    fn render_status_modal(&self, frame: &mut Frame, area: Rect, selected: usize) {
        use ratatui::widgets::Clear;

        let modal_width = 30u16;
        let modal_height = (ALL_STATUSES.len() as u16) + 2; // +2 for border
        let x = area.x + (area.width.saturating_sub(modal_width)) / 2;
        let y = area.y + (area.height.saturating_sub(modal_height)) / 2;
        let modal_area = Rect::new(x, y, modal_width, modal_height);

        frame.render_widget(Clear, modal_area);

        let items: Vec<ListItem> = ALL_STATUSES
            .iter()
            .enumerate()
            .map(|(i, status)| {
                let marker = if i == selected { "▶ " } else { "  " };
                ListItem::new(format!("{marker}{status}"))
            })
            .collect();

        let mut modal_list_state = ListState::default();
        modal_list_state.select(Some(selected));

        let list = List::new(items)
            .block(
                Block::default()
                    .title(" Set Status ")
                    .borders(Borders::ALL)
                    .style(Style::default().bg(Color::Black)),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            );

        frame.render_stateful_widget(list, modal_area, &mut modal_list_state);
    }

    /// Render a text-input modal centered on screen.
    fn render_input_modal(frame: &mut Frame, area: Rect, title: &str, input: &str) {
        use ratatui::widgets::Clear;

        let modal_width = (area.width / 2).max(30).min(area.width);
        let modal_height = 3u16;
        let x = area.x + (area.width.saturating_sub(modal_width)) / 2;
        let y = area.y + (area.height.saturating_sub(modal_height)) / 2;
        let modal_area = Rect::new(x, y, modal_width, modal_height);

        frame.render_widget(Clear, modal_area);

        // Scroll input when it exceeds the visible inner width (borders take 2 cols)
        let inner_width = modal_width.saturating_sub(2) as usize;
        let display_text = if input.len() > inner_width && inner_width > 1 {
            let tail_len = inner_width - 1; // 1 char for '…'
            let start = input.len() - tail_len;
            format!("…{}", &input[start..])
        } else {
            input.to_string()
        };

        let paragraph = Paragraph::new(display_text)
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .style(Style::default().bg(Color::Black)),
            )
            .style(Style::default().fg(Color::White));
        frame.render_widget(paragraph, modal_area);
    }

    /// Render a text-input prompt at the bottom of the screen.
    fn render_input_prompt(frame: &mut Frame, area: Rect, title: &str, input: &str) {
        use ratatui::widgets::Clear;

        let input_area = Rect::new(
            area.x,
            area.y + area.height.saturating_sub(3),
            area.width,
            3,
        );
        frame.render_widget(Clear, input_area);

        let paragraph = Paragraph::new(input.to_string())
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .style(Style::default().bg(Color::Black)),
            )
            .style(Style::default().fg(Color::White));
        frame.render_widget(paragraph, input_area);
    }
}

fn priority_color(p: Priority) -> Color {
    match p {
        Priority::P0 => Color::Red,
        Priority::P1 => Color::Yellow,
        Priority::P2 => Color::Blue,
        Priority::P3 => Color::DarkGray,
    }
}

fn status_indicator(s: &Status) -> &'static str {
    match s {
        Status::Open => "○",
        Status::InProgress => "●",
        Status::Done => "✓",
        Status::Blocked => "⊘",
        Status::Cancelled => "✗",
    }
}

/// Recursively render a dependency tree node into Lines.
fn render_dep_tree<'a>(node: &DepNode<'a>, prefix: &str, last: bool, lines: &mut Vec<Line<'a>>) {
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
        render_dep_tree(child, &child_prefix, i == child_count - 1, lines);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{Priority, Task};
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use std::collections::HashMap;

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn sample_tasks() -> (Vec<Task>, HashMap<String, Task>) {
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

    fn make_app() -> App {
        let (tasks, map) = sample_tasks();
        App::new(tasks, map, std::path::PathBuf::from("."))
    }

    #[test]
    fn test_initial_selection() {
        let app = make_app();
        assert_eq!(app.selected_index(), Some(0));
        assert!(app.selected_task().is_some());
    }

    #[test]
    fn test_empty_app() {
        let app = App::new(vec![], HashMap::new(), std::path::PathBuf::from("."));
        assert_eq!(app.selected_index(), None);
        assert!(app.selected_task().is_none());
    }

    #[test]
    fn test_navigate_down_wraps() {
        let mut app = make_app();
        let visible = app.tasks.len();
        for _ in 0..visible {
            app.handle_key(make_key(KeyCode::Char('j')));
        }
        assert_eq!(app.selected_index(), Some(0));
    }

    #[test]
    fn test_navigate_up_wraps() {
        let mut app = make_app();
        app.handle_key(make_key(KeyCode::Char('k')));
        assert_eq!(app.selected_index(), Some(app.tasks.len() - 1));
    }

    #[test]
    fn test_quit_action() {
        let mut app = make_app();
        let action = app.handle_key(make_key(KeyCode::Char('q')));
        assert!(matches!(action, Action::Quit));
    }

    #[test]
    fn test_esc_quits_from_normal() {
        let mut app = make_app();
        let action = app.handle_key(make_key(KeyCode::Esc));
        assert!(matches!(action, Action::Quit));
    }

    #[test]
    fn test_edit_action() {
        let mut app = make_app();
        let action = app.handle_key(make_key(KeyCode::Char('e')));
        assert!(matches!(action, Action::EditSelected));
    }

    #[test]
    fn test_create_mode() {
        let mut app = make_app();
        let action = app.handle_key(make_key(KeyCode::Char('c')));
        assert!(matches!(action, Action::None));
        assert!(matches!(app.mode, Mode::CreateInput { .. }));

        app.handle_key(make_key(KeyCode::Char('H')));
        app.handle_key(make_key(KeyCode::Char('i')));

        let action = app.handle_key(make_key(KeyCode::Enter));
        assert!(matches!(action, Action::CreateTask(ref t) if t == "Hi"));
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn test_create_cancel() {
        let mut app = make_app();
        app.handle_key(make_key(KeyCode::Char('c')));
        app.handle_key(make_key(KeyCode::Char('x')));
        let action = app.handle_key(make_key(KeyCode::Esc));
        assert!(matches!(action, Action::None));
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn test_create_empty_title_no_action() {
        let mut app = make_app();
        app.handle_key(make_key(KeyCode::Char('c')));
        let action = app.handle_key(make_key(KeyCode::Enter));
        assert!(matches!(action, Action::None));
    }

    #[test]
    fn test_status_select_mode() {
        let mut app = make_app();
        app.handle_key(make_key(KeyCode::Char('s')));
        assert!(matches!(app.mode, Mode::StatusSelect { .. }));

        app.handle_key(make_key(KeyCode::Char('j')));
        let action = app.handle_key(make_key(KeyCode::Enter));
        assert!(matches!(action, Action::UpdateStatus(_, _)));
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn test_status_cancel() {
        let mut app = make_app();
        app.handle_key(make_key(KeyCode::Char('s')));
        let action = app.handle_key(make_key(KeyCode::Esc));
        assert!(matches!(action, Action::None));
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn test_filter_text() {
        let mut app = make_app();
        app.handle_key(make_key(KeyCode::Char('/')));
        assert!(matches!(app.mode, Mode::FilterInput { .. }));

        // Filter by "aaa" (task ID) for exact match
        app.handle_key(make_key(KeyCode::Char('a')));
        app.handle_key(make_key(KeyCode::Char('a')));
        app.handle_key(make_key(KeyCode::Char('a')));
        app.handle_key(make_key(KeyCode::Enter));

        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.tasks.len(), 1);
        assert_eq!(app.tasks[0].id, "aaa");
    }

    #[test]
    fn test_filter_show_all() {
        let mut app = make_app();
        let before = app.tasks.len();
        app.handle_key(make_key(KeyCode::Char('a')));
        assert!(app.filter.show_all);
        assert!(app.tasks.len() >= before);
    }

    #[test]
    fn test_filter_default_hides_done() {
        let app = make_app();
        assert!(!app.filter.show_all);
        let has_done = app.tasks.iter().any(|t| t.status == Status::Done);
        assert!(!has_done);
    }

    #[test]
    fn test_backspace_in_create() {
        let mut app = make_app();
        app.handle_key(make_key(KeyCode::Char('c')));
        app.handle_key(make_key(KeyCode::Char('A')));
        app.handle_key(make_key(KeyCode::Char('B')));
        app.handle_key(make_key(KeyCode::Backspace));
        let action = app.handle_key(make_key(KeyCode::Enter));
        assert!(matches!(action, Action::CreateTask(ref t) if t == "A"));
    }

    #[test]
    fn test_home_end_navigation() {
        let mut app = make_app();
        app.handle_key(make_key(KeyCode::End));
        assert_eq!(app.selected_index(), Some(app.tasks.len() - 1));
        app.handle_key(make_key(KeyCode::Home));
        assert_eq!(app.selected_index(), Some(0));
    }

    // ── Grapheme/Unicode input behavior ──────────────────────────────────

    #[test]
    fn test_backspace_ascii() {
        // Standard ASCII backspace removes one character
        let result = App::handle_text_input_key(make_key(KeyCode::Backspace), "abc");
        assert_eq!(result, Some(Some("ab".to_string())));
    }

    #[test]
    fn test_backspace_accented_chars() {
        // Accented characters (single codepoint) are removed correctly
        let result = App::handle_text_input_key(make_key(KeyCode::Backspace), "café");
        assert_eq!(result, Some(Some("caf".to_string())));
    }

    #[test]
    fn test_backspace_cjk() {
        // CJK characters (single codepoint) are removed correctly
        let result = App::handle_text_input_key(make_key(KeyCode::Backspace), "任務");
        assert_eq!(result, Some(Some("任".to_string())));
    }

    #[test]
    fn test_backspace_simple_emoji() {
        // Simple emoji (single codepoint) are removed correctly
        let result = App::handle_text_input_key(make_key(KeyCode::Backspace), "hi🎉");
        assert_eq!(result, Some(Some("hi".to_string())));
    }

    #[test]
    fn test_backspace_empty_string() {
        let result = App::handle_text_input_key(make_key(KeyCode::Backspace), "");
        assert_eq!(result, Some(Some("".to_string())));
    }

    #[test]
    fn test_input_multibyte_char() {
        // Typing a multibyte character works
        let result = App::handle_text_input_key(make_key(KeyCode::Char('é')), "caf");
        assert_eq!(result, Some(Some("café".to_string())));
    }

    #[test]
    fn test_delete_confirm_and_cancel() {
        let mut app = make_app();
        app.handle_key(make_key(KeyCode::Char('d')));
        assert!(matches!(app.mode, Mode::ConfirmDelete { .. }));

        // Any key other than 'y' cancels
        app.handle_key(make_key(KeyCode::Char('n')));
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn test_delete_confirm_y() {
        let mut app = make_app();
        app.handle_key(make_key(KeyCode::Char('d')));
        let action = app.handle_key(make_key(KeyCode::Char('y')));
        assert!(matches!(action, Action::DeleteTask(_)));
        assert_eq!(app.mode, Mode::Normal);
    }

    // ── Focus & scroll tests ─────────────────────────────────────────────

    #[test]
    fn test_initial_focus_is_list() {
        let app = make_app();
        assert_eq!(app.focus, FocusPane::List);
        assert_eq!(app.detail_scroll, 0);
    }

    #[test]
    fn test_tab_toggles_focus() {
        let mut app = make_app();
        assert_eq!(app.focus, FocusPane::List);
        app.handle_key(make_key(KeyCode::Tab));
        assert_eq!(app.focus, FocusPane::Detail);
        app.handle_key(make_key(KeyCode::Tab));
        assert_eq!(app.focus, FocusPane::List);
    }

    #[test]
    fn test_jk_navigates_list_when_list_focused() {
        let mut app = make_app();
        assert_eq!(app.focus, FocusPane::List);
        assert_eq!(app.selected_index(), Some(0));
        app.handle_key(make_key(KeyCode::Char('j')));
        assert_eq!(app.selected_index(), Some(1));
        app.handle_key(make_key(KeyCode::Char('k')));
        assert_eq!(app.selected_index(), Some(0));
    }

    #[test]
    fn test_jk_scrolls_detail_when_detail_focused() {
        let mut app = make_app();
        // Simulate rendered dimensions: content taller than viewport
        app.detail_content_height = 50;
        app.detail_visible_height = 10;
        app.handle_key(make_key(KeyCode::Tab));
        assert_eq!(app.focus, FocusPane::Detail);
        assert_eq!(app.detail_scroll, 0);

        app.handle_key(make_key(KeyCode::Char('j')));
        assert_eq!(app.detail_scroll, 1);
        app.handle_key(make_key(KeyCode::Char('j')));
        assert_eq!(app.detail_scroll, 2);
        app.handle_key(make_key(KeyCode::Char('k')));
        assert_eq!(app.detail_scroll, 1);
    }

    #[test]
    fn test_detail_scroll_does_not_underflow() {
        let mut app = make_app();
        app.handle_key(make_key(KeyCode::Tab));
        app.handle_key(make_key(KeyCode::Char('k')));
        assert_eq!(app.detail_scroll, 0);
    }

    #[test]
    fn test_scroll_resets_on_task_change() {
        let mut app = make_app();
        app.detail_content_height = 50;
        app.detail_visible_height = 10;
        // Scroll detail
        app.handle_key(make_key(KeyCode::Tab));
        app.handle_key(make_key(KeyCode::Char('j')));
        app.handle_key(make_key(KeyCode::Char('j')));
        assert_eq!(app.detail_scroll, 2);

        // Switch to list and navigate to different task
        app.handle_key(make_key(KeyCode::Tab));
        app.handle_key(make_key(KeyCode::Char('j')));
        assert_eq!(app.detail_scroll, 0);
    }

    #[test]
    fn test_home_resets_detail_scroll() {
        let mut app = make_app();
        app.detail_content_height = 50;
        app.detail_visible_height = 10;
        app.handle_key(make_key(KeyCode::Tab));
        app.handle_key(make_key(KeyCode::Char('j')));
        app.handle_key(make_key(KeyCode::Char('j')));
        assert_eq!(app.detail_scroll, 2);
        app.handle_key(make_key(KeyCode::Home));
        assert_eq!(app.detail_scroll, 0);
    }

    #[test]
    fn test_existing_keys_work_in_detail_focus() {
        let mut app = make_app();
        app.handle_key(make_key(KeyCode::Tab));
        assert_eq!(app.focus, FocusPane::Detail);

        // 'e' should still trigger edit
        let action = app.handle_key(make_key(KeyCode::Char('e')));
        assert!(matches!(action, Action::EditSelected));

        // 'c' should enter create mode
        let action = app.handle_key(make_key(KeyCode::Char('c')));
        assert!(matches!(action, Action::None));
        assert!(matches!(app.mode, Mode::CreateInput { .. }));
    }

    #[test]
    fn test_enter_moves_focus_to_detail() {
        let mut app = make_app();
        assert_eq!(app.focus, FocusPane::List);
        app.handle_key(make_key(KeyCode::Enter));
        assert_eq!(app.focus, FocusPane::Detail);
    }
}
