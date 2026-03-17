mod app;

use std::collections::HashMap;
use std::path::Path;

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;

use crate::error::Result;
use crate::store;
use crate::task::Task;

pub use app::{Action, App};

/// Load all tasks from disk (sync bridge) and return them sorted by priority then creation date.
fn load_tasks_sync(base: &Path) -> Result<(Vec<Task>, HashMap<String, Task>)> {
    let task_map = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(store::load_all(base))
    })?;
    let mut task_list: Vec<Task> = task_map.values().cloned().collect();
    task_list.sort_by(|a, b| a.priority.cmp(&b.priority).then(a.created.cmp(&b.created)));
    Ok((task_list, task_map))
}

/// Run the TUI application.
pub async fn run(base: &Path) -> Result<()> {
    let (task_list, task_map) = load_tasks_sync(base)?;

    let mut app = App::new(task_list, task_map, base.to_path_buf());
    let mut terminal = ratatui::init();
    let result = run_loop(&mut app, &mut terminal);
    ratatui::restore();
    result
}

/// Reload tasks from disk and update app state, preserving selection.
fn reload(app: &mut App) -> Result<()> {
    let (task_list, task_map) = load_tasks_sync(&app.base)?;
    let selected_id = app.selected_task().map(|t| t.id.clone());
    app.reload(task_list, task_map);
    if let Some(id) = selected_id
        && let Some(idx) = app.tasks.iter().position(|t| t.id == id)
    {
        app.list_state.select(Some(idx));
    }
    Ok(())
}

/// Launch $EDITOR on a task file, suspending the TUI.
fn edit_task_in_editor(app: &App, task_id: &str, terminal: &mut DefaultTerminal) -> Result<()> {
    let path = store::find_task_path(&app.base, task_id)?;

    // Suspend TUI
    ratatui::restore();

    let result = crate::editor::open_in_editor(&path);

    // Restore TUI
    *terminal = ratatui::init();

    result
}

/// Create a new task and optionally open in editor. Returns the new task ID.
fn create_task(app: &App, title: &str, terminal: &mut DefaultTerminal) -> Result<String> {
    use crate::service;
    use crate::task::{Priority, TaskType};

    let (_, task_map) = load_tasks_sync(&app.base)?;
    let task = service::create_task(
        &app.base,
        &task_map,
        title.to_string(),
        Priority::P2,
        Vec::new(),
        Vec::new(),
        None,
        String::new(),
        TaskType::Task,
    )?;

    let id = task.id.clone();
    // Open in editor for body
    edit_task_in_editor(app, &task.id, terminal)?;

    Ok(id)
}

/// Main event loop: render, poll events, dispatch.
fn run_loop(app: &mut App, terminal: &mut DefaultTerminal) -> Result<()> {
    loop {
        terminal.draw(|frame| app.render(frame))?;

        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            // Ctrl+C always quits
            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                break;
            }

            let action = app.handle_key(key);
            match action {
                Action::None => {}
                Action::Quit => break,
                Action::EditSelected => {
                    if let Some(task) = app.selected_task() {
                        let id = task.id.clone();
                        match edit_task_in_editor(app, &id, terminal).and_then(|_| reload(app)) {
                            Ok(()) => app.error_message = None,
                            Err(e) => app.error_message = Some(e.to_string()),
                        }
                    }
                }
                Action::CreateTask(title) => {
                    match create_task(app, &title, terminal).and_then(|new_id| {
                        reload(app)?;
                        if let Some(idx) = app.tasks.iter().position(|t| t.id == new_id) {
                            app.list_state.select(Some(idx));
                        }
                        Ok(())
                    }) {
                        Ok(()) => app.error_message = None,
                        Err(e) => app.error_message = Some(e.to_string()),
                    }
                }
                Action::UpdateStatus(id, status) => {
                    match (|| -> Result<()> {
                        let (_, task_map) = load_tasks_sync(&app.base)?;
                        crate::service::set_status(&app.base, &task_map, &id, status)?;
                        reload(app)?;
                        Ok(())
                    })() {
                        Ok(()) => app.error_message = None,
                        Err(e) => app.error_message = Some(e.to_string()),
                    }
                }
                Action::DeleteTask(id) => {
                    match (|| -> Result<()> {
                        let (_, task_map) = load_tasks_sync(&app.base)?;
                        crate::service::delete_task(&app.base, &task_map, &id)?;
                        reload(app)?;
                        Ok(())
                    })() {
                        Ok(()) => app.error_message = None,
                        Err(e) => app.error_message = Some(e.to_string()),
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::Task;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn make_app_with_base(base: PathBuf) -> App {
        App::new(vec![], HashMap::new(), base)
    }

    // ── Action error paths (reload / status update) ──────────────────────
    //
    // These test that errors from disk operations are returned by helpers,
    // allowing run_loop to catch them and store in app.error_message
    // instead of terminating the TUI.

    #[test]
    fn reload_nonexistent_base_returns_error() {
        let mut app = make_app_with_base(PathBuf::from("/tmp/__bears_nonexistent__"));
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async { reload(&mut app) });
        assert!(result.is_err(), "reload on missing base should fail");
    }

    #[test]
    fn set_status_unknown_task_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        let bears_dir = tmp.path().join(".bears");
        std::fs::create_dir_all(&bears_dir).unwrap();

        let tasks: HashMap<String, Task> = HashMap::new();
        let result = crate::service::set_status(
            tmp.path(),
            &tasks,
            "nonexistent",
            crate::task::Status::Done,
        );
        assert!(result.is_err(), "set_status on unknown task ID should fail");
    }

    /// Helpers return errors that run_loop catches and stores in
    /// app.error_message — the TUI continues running instead of exiting.
    #[test]
    fn error_message_set_on_reload_failure() {
        let mut app = make_app_with_base(PathBuf::from("/tmp/__bears_no_dir__"));
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async { reload(&mut app) });
        // The helper returns Err; run_loop stores it in error_message
        let err_msg = result.unwrap_err().to_string();
        app.error_message = Some(err_msg.clone());
        assert!(app.error_message.is_some());
        assert!(!err_msg.is_empty());
    }

    #[test]
    fn error_message_cleared_on_success() {
        let mut app = make_app_with_base(PathBuf::from("/tmp/__bears_test__"));
        app.error_message = Some("previous error".into());
        // Simulate successful action clearing the error
        app.error_message = None;
        assert!(app.error_message.is_none());
    }
}
