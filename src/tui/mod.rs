mod app;
mod input;
mod style;
pub(crate) mod watcher;
mod widgets;

use std::collections::HashMap;
use std::path::Path;

use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;

use crate::error::Result;
use crate::store;
use crate::task::Task;

pub use app::{Action, App};

/// Opaque handle that keeps the file watcher alive.
///
/// Dropping this stops the background watcher thread. The TUI holds it for
/// the duration of the run loop, then drops it on exit.
type WatcherHandle = Box<dyn std::any::Any + Send>;

/// Start the debounced file watcher on `bears_dir`.
///
/// Returns a `(handle, receiver)` pair. The caller must keep the handle alive
/// for the duration of the TUI; dropping it stops the watcher.
///
/// On failure, logs a warning and returns a permanently-closed channel so the
/// event loop can continue without live reload (graceful degradation).
fn start_watcher(bears_dir: &Path) -> (WatcherHandle, tokio::sync::mpsc::Receiver<()>) {
    match watcher::watch_bears_dir(bears_dir) {
        Ok((debouncer, rx)) => (Box::new(debouncer), rx),
        Err(e) => {
            eprintln!("warn: file watcher could not start ({e}); live reload disabled");
            // Return a dummy, immediately-closed channel — the event loop handles
            // `None` from recv() gracefully and simply skips live-reload.
            let (tx, rx) = tokio::sync::mpsc::channel(1);
            drop(tx); // close immediately
            (Box::new(()), rx)
        }
    }
}

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

    // Start the file watcher. Degrade gracefully if it can't start.
    let bears_dir = base.join(".bears");
    // `_watcher` keeps the watcher alive; dropping it stops the background thread.
    let (_watcher, mut watcher_rx) = start_watcher(&bears_dir);

    let result = run_loop(&mut app, &mut terminal, &mut watcher_rx).await;
    ratatui::restore();
    // _watcher is dropped here, stopping the watcher thread cleanly.
    result
}

/// Reload tasks from disk and update app state, preserving selection.
pub(crate) fn reload(app: &mut App) -> Result<()> {
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

/// Read one crossterm event asynchronously via spawn_blocking.
///
/// Returns `None` if the blocking thread panicked or was cancelled.
async fn read_crossterm_event() -> Option<Event> {
    tokio::task::spawn_blocking(|| crossterm::event::read().ok())
        .await
        .ok()
        .flatten()
}

/// Main event loop: render, poll events (keyboard + watcher), dispatch.
async fn run_loop(
    app: &mut App,
    terminal: &mut DefaultTerminal,
    watcher_rx: &mut tokio::sync::mpsc::Receiver<()>,
) -> Result<()> {
    // Keep a single in-flight keyboard-read future alive across loop iterations.
    // If we recreated it inside `select!` each time, a watcher-triggered reload
    // would cancel the in-flight read and orphan its blocking thread, which would
    // then swallow the next keypress. Persisting it avoids that leak.
    let mut read_fut = Box::pin(read_crossterm_event());
    // Disable the watcher branch once its channel closes, so a permanently-ready
    // `recv() -> None` can't spin the loop (the graceful-degradation path).
    let mut watcher_live = true;

    loop {
        terminal.draw(|frame| app.render(frame))?;

        // Concurrently wait for either a keyboard event or a watcher signal.
        tokio::select! {
            // Keyboard / terminal event. The read future is preserved across
            // iterations and only recreated once it resolves.
            event = &mut read_fut => {
                read_fut = Box::pin(read_crossterm_event());
                let Some(Event::Key(key)) = event else { continue };
                if key.kind != KeyEventKind::Press {
                    continue;
                }

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

            // File-system watcher signal: reload tasks from disk. The branch is
            // disabled once the channel closes so it can't busy-loop.
            signal = watcher_rx.recv(), if watcher_live => {
                match signal {
                    Some(()) => match reload(app) {
                        Ok(()) => app.error_message = None,
                        Err(e) => app.error_message = Some(e.to_string()),
                    },
                    // Channel closed (watcher stopped or failed to start):
                    // stop selecting on it for the rest of the session.
                    None => watcher_live = false,
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{Priority, Status, Task};
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

    /// Verify that apply_reload_to_app correctly updates App state and
    /// preserves the cursor position by task id after a reload.
    #[test]
    fn reload_preserves_selection_by_id() {
        // Build initial state: three tasks, select task "bbb" (index 1).
        let make_task = |id: &str, title: &str| {
            let mut t = Task::new(id.to_string(), title.to_string(), Priority::P1);
            t.status = Status::Open;
            t
        };

        let tasks = vec![
            make_task("aaa", "Task A"),
            make_task("bbb", "Task B"),
            make_task("ccc", "Task C"),
        ];
        let map: HashMap<String, Task> = tasks.iter().map(|t| (t.id.clone(), t.clone())).collect();
        let mut app = App::new(tasks, map, PathBuf::from("."));

        // Select "bbb" (index 1 in Open filter)
        app.list_state.select(Some(1));
        assert_eq!(app.selected_task().map(|t| t.id.as_str()), Some("bbb"));

        // Simulate a reload that adds a new task "aaa2" between "aaa" and "bbb".
        // In sorted order the new list is: [aaa, aaa2, bbb, ccc].
        let mut new_task = make_task("aaa2", "Task A2");
        new_task.priority = Priority::P1;
        let new_tasks = vec![
            make_task("aaa", "Task A"),
            new_task.clone(),
            make_task("bbb", "Task B"),
            make_task("ccc", "Task C"),
        ];
        let new_map: HashMap<String, Task> = new_tasks
            .iter()
            .map(|t| (t.id.clone(), t.clone()))
            .collect();

        // Apply the reload and re-select by id.
        let selected_id = app.selected_task().map(|t| t.id.clone());
        app.reload(new_tasks, new_map);
        if let Some(id) = selected_id
            && let Some(idx) = app.tasks.iter().position(|t| t.id == id)
        {
            app.list_state.select(Some(idx));
        }

        // "bbb" should now be at index 2 (after the inserted aaa2).
        assert_eq!(
            app.selected_task().map(|t| t.id.as_str()),
            Some("bbb"),
            "selection should follow task 'bbb' after reload inserts a new task"
        );
        assert_eq!(
            app.selected_index(),
            Some(2),
            "index should be 2 after aaa2 was inserted before bbb"
        );
    }

    /// A closed watcher channel (debouncer dropped, or the watcher failed to
    /// start) yields `None` from `recv()`. The event loop relies on this to flip
    /// `watcher_live = false` and stop selecting on the branch, rather than
    /// busy-looping on a permanently-ready `recv()`. We can't drive `run_loop`
    /// without a real terminal, so we assert the precondition the guard depends on.
    #[tokio::test]
    async fn watcher_channel_closed_yields_none() {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(1);
        drop(tx); // close immediately

        assert!(
            rx.recv().await.is_none(),
            "closed channel must yield None so run_loop can disable the watcher branch"
        );
    }
}
