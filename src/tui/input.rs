use crossterm::event::{KeyCode, KeyEvent};

use super::app::{Action, App, FocusPane, Mode};
use super::style::ALL_STATUSES;

/// Result of resolving a text-input key event.
enum TextInputResult {
    /// User pressed Enter — submit the trimmed input.
    Submit(String),
    /// Input was updated (character typed or backspace).
    Update(String),
    /// User pressed Esc — cancel input.
    Cancel,
    /// Key was not handled by text input logic.
    Ignored,
}

/// Resolve a key event against the current text input. Handles Enter, Esc, Backspace, Char.
fn resolve_text_input(key: KeyEvent, input: &str) -> TextInputResult {
    match key.code {
        KeyCode::Enter => TextInputResult::Submit(input.trim().to_string()),
        KeyCode::Esc => TextInputResult::Cancel,
        KeyCode::Backspace => {
            let mut s = input.to_string();
            s.pop();
            TextInputResult::Update(s)
        }
        KeyCode::Char(c) => {
            let mut s = input.to_string();
            s.push(c);
            TextInputResult::Update(s)
        }
        _ => TextInputResult::Ignored,
    }
}

impl App {
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
                    FocusPane::Detail => {
                        self.detail_scroll = self.detail_scroll.saturating_sub(1);
                    }
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
                    FocusPane::Detail => { /* ratatui handles end clamp */ }
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

    fn handle_create_input_key(&mut self, key: KeyEvent) -> Action {
        let input = match &self.mode {
            Mode::CreateInput { input } => input.clone(),
            _ => return Action::None,
        };

        match resolve_text_input(key, &input) {
            TextInputResult::Submit(title) => {
                self.mode = Mode::Normal;
                if title.is_empty() {
                    Action::None
                } else {
                    Action::CreateTask(title)
                }
            }
            TextInputResult::Update(new_input) => {
                self.mode = Mode::CreateInput { input: new_input };
                Action::None
            }
            TextInputResult::Cancel => {
                self.mode = Mode::Normal;
                Action::None
            }
            TextInputResult::Ignored => Action::None,
        }
    }

    fn handle_filter_input_key(&mut self, key: KeyEvent) -> Action {
        let input = match &self.mode {
            Mode::FilterInput { input } => input.clone(),
            _ => return Action::None,
        };

        match resolve_text_input(key, &input) {
            TextInputResult::Submit(query) => {
                self.filter.query = query;
                self.mode = Mode::Normal;
                self.apply_filter();
                Action::None
            }
            TextInputResult::Update(new_input) => {
                self.mode = Mode::FilterInput { input: new_input };
                Action::None
            }
            TextInputResult::Cancel => {
                self.mode = Mode::Normal;
                Action::None
            }
            TextInputResult::Ignored => Action::None,
        }
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
                self.mode = Mode::Normal;
                Action::None
            }
        }
    }

    pub(super) fn select_next(&mut self) {
        if self.tasks.is_empty() {
            return;
        }
        let i = self.selected_index().unwrap_or(0);
        let next = if i >= self.tasks.len() - 1 { 0 } else { i + 1 };
        self.list_state.select(Some(next));
    }

    pub(super) fn select_prev(&mut self) {
        if self.tasks.is_empty() {
            return;
        }
        let i = self.selected_index().unwrap_or(0);
        let prev = if i == 0 { self.tasks.len() - 1 } else { i - 1 };
        self.list_state.select(Some(prev));
    }

    pub(super) fn select_first(&mut self) {
        if !self.tasks.is_empty() {
            self.list_state.select(Some(0));
        }
    }

    pub(super) fn select_last(&mut self) {
        if !self.tasks.is_empty() {
            self.list_state.select(Some(self.tasks.len() - 1));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::app::test_helpers::{make_app, make_key};
    use super::*;
    use crossterm::event::KeyCode;

    #[test]
    fn test_initial_selection() {
        let app = make_app();
        assert_eq!(app.selected_index(), Some(0));
        assert!(app.selected_task().is_some());
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

    #[test]
    fn test_delete_confirm_and_cancel() {
        let mut app = make_app();
        app.handle_key(make_key(KeyCode::Char('d')));
        assert!(matches!(app.mode, Mode::ConfirmDelete { .. }));

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
        app.handle_key(make_key(KeyCode::Tab));
        app.handle_key(make_key(KeyCode::Char('j')));
        app.handle_key(make_key(KeyCode::Char('j')));
        assert_eq!(app.detail_scroll, 2);

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

        let action = app.handle_key(make_key(KeyCode::Char('e')));
        assert!(matches!(action, Action::EditSelected));

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

    // ── Text input resolve tests ─────────────────────────────────────────

    #[test]
    fn test_resolve_text_input_submit() {
        let result = resolve_text_input(make_key(KeyCode::Enter), "  hello  ");
        assert!(matches!(result, TextInputResult::Submit(s) if s == "hello"));
    }

    #[test]
    fn test_resolve_text_input_cancel() {
        let result = resolve_text_input(make_key(KeyCode::Esc), "anything");
        assert!(matches!(result, TextInputResult::Cancel));
    }

    #[test]
    fn test_resolve_text_input_backspace() {
        let result = resolve_text_input(make_key(KeyCode::Backspace), "abc");
        assert!(matches!(result, TextInputResult::Update(s) if s == "ab"));
    }

    #[test]
    fn test_resolve_text_input_char() {
        let result = resolve_text_input(make_key(KeyCode::Char('x')), "ab");
        assert!(matches!(result, TextInputResult::Update(s) if s == "abx"));
    }
}
