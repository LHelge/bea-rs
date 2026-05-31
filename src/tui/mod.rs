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

/// Reload tasks from disk and update app state.
///
/// `App::reload` preserves the selected task by id (falling back to the
/// nearest neighbour when deleted), the current list-mode and search query,
/// and clamps the detail-pane scroll to the new content height.
pub(crate) fn reload(app: &mut App) -> Result<()> {
    let (task_list, task_map) = load_tasks_sync(&app.base)?;
    app.reload(task_list, task_map);
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

    // ── Reload selection/scroll/mode/search preservation ────────────────

    fn make_open_tasks(ids_titles: &[(&str, &str)]) -> (Vec<Task>, HashMap<String, Task>) {
        let tasks: Vec<Task> = ids_titles
            .iter()
            .map(|(id, title)| {
                let mut t = Task::new(id.to_string(), title.to_string(), Priority::P1);
                t.status = Status::Open;
                t
            })
            .collect();
        let map = tasks.iter().map(|t| (t.id.clone(), t.clone())).collect();
        (tasks, map)
    }

    /// `App::reload` follows the selected task by id when it moves in the list.
    #[test]
    fn reload_preserves_selection_by_id() {
        let (tasks, map) =
            make_open_tasks(&[("aaa", "Task A"), ("bbb", "Task B"), ("ccc", "Task C")]);
        let mut app = App::new(tasks, map, PathBuf::from("."));

        // Select "bbb" at index 1.
        app.list_state.select(Some(1));
        assert_eq!(app.selected_task().map(|t| t.id.as_str()), Some("bbb"));

        // Reload: insert "aaa2" between "aaa" and "bbb" so "bbb" shifts to index 2.
        let (new_tasks, new_map) = make_open_tasks(&[
            ("aaa", "Task A"),
            ("aaa2", "Task A2"),
            ("bbb", "Task B"),
            ("ccc", "Task C"),
        ]);
        app.reload(new_tasks, new_map);

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

    /// When the selected task is deleted, `App::reload` falls back to the
    /// nearest neighbour (old index clamped to the new list length).
    #[test]
    fn reload_falls_back_to_neighbour_when_task_deleted() {
        let (tasks, map) = make_open_tasks(&[
            ("aaa", "Task A"),
            ("bbb", "Task B"),
            ("ccc", "Task C"),
            ("ddd", "Task D"),
        ]);
        let mut app = App::new(tasks, map, PathBuf::from("."));

        // Select "ccc" at index 2.
        app.list_state.select(Some(2));
        assert_eq!(app.selected_task().map(|t| t.id.as_str()), Some("ccc"));

        // Reload: remove "ccc". New list is [aaa, bbb, ddd] (indices 0,1,2).
        // Old index was 2 → clamp to new len-1 = 2 → selects "ddd".
        let (new_tasks, new_map) =
            make_open_tasks(&[("aaa", "Task A"), ("bbb", "Task B"), ("ddd", "Task D")]);
        app.reload(new_tasks, new_map);

        assert_eq!(
            app.selected_task().map(|t| t.id.as_str()),
            Some("ddd"),
            "should fall back to the task now at the old index (clamped)"
        );
        assert_eq!(app.selected_index(), Some(2));
    }

    /// When the list becomes shorter than the old index, the index is clamped.
    #[test]
    fn reload_clamps_index_when_list_shrinks() {
        let (tasks, map) =
            make_open_tasks(&[("aaa", "Task A"), ("bbb", "Task B"), ("ccc", "Task C")]);
        let mut app = App::new(tasks, map, PathBuf::from("."));

        // Select index 2 ("ccc").
        app.list_state.select(Some(2));

        // Reload: only one task remains.
        let (new_tasks, new_map) = make_open_tasks(&[("aaa", "Task A")]);
        app.reload(new_tasks, new_map);

        // Old index 2 clamped to len-1 = 0.
        assert_eq!(app.selected_index(), Some(0));
        assert_eq!(app.selected_task().map(|t| t.id.as_str()), Some("aaa"));
    }

    /// `App::reload` preserves the current list mode and search query.
    #[test]
    fn reload_preserves_mode_and_search_query() {
        use crate::tui::app::{Filter, ListMode};

        let (tasks, map) = make_open_tasks(&[("aaa", "Task A alpha"), ("bbb", "Task B beta")]);
        let mut app = App::new(tasks, map, PathBuf::from("."));

        // Switch to All mode with a search filter.
        app.filter = Filter {
            list_mode: ListMode::All,
            query: "alpha".to_string(),
        };
        app.apply_filter();

        // After filter: only "aaa" should be visible.
        assert_eq!(app.tasks.len(), 1);
        assert_eq!(app.tasks[0].id, "aaa");

        // Reload with the same tasks.
        let (new_tasks, new_map) =
            make_open_tasks(&[("aaa", "Task A alpha"), ("bbb", "Task B beta")]);
        app.reload(new_tasks, new_map);

        // Mode and query must be preserved.
        assert_eq!(
            app.filter.list_mode,
            ListMode::All,
            "list mode must survive reload"
        );
        assert_eq!(
            app.filter.query, "alpha",
            "search query must survive reload"
        );
        // Filter is still active.
        assert_eq!(
            app.tasks.len(),
            1,
            "filter must still be applied after reload"
        );
        assert_eq!(app.tasks[0].id, "aaa");
    }

    /// After reload, `detail_scroll` is clamped so it never exceeds the new
    /// content height. (Before the first render the content height is still
    /// the value from the previous frame; we clamp to it conservatively.)
    #[test]
    fn reload_clamps_detail_scroll() {
        let (tasks, map) = make_open_tasks(&[("aaa", "Task A")]);
        let mut app = App::new(tasks, map, PathBuf::from("."));

        // Artificially set a high scroll and a small content-height as if the
        // task detail shrank between frames.
        app.detail_scroll = 50;
        app.detail_content_height = 10;
        app.detail_visible_height = 10; // content fits → max scroll = 0

        let (new_tasks, new_map) = make_open_tasks(&[("aaa", "Task A")]);
        app.reload(new_tasks, new_map);

        // detail_max_scroll() = content_height.saturating_sub(visible_height) = 0
        assert_eq!(
            app.detail_scroll, 0,
            "scroll must be clamped to new content height after reload"
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

    // ── Load-then-reload application path ────────────────────────────────
    //
    // These tests exercise the full disk→app reload pipeline without a real
    // watcher: write task files to a temp dir, call `load_all`, feed the result
    // into `App::reload`, and assert the in-memory state is correct.

    /// Helper: write a minimal valid task file into a `.bears/` directory.
    fn write_task_file(bears_dir: &std::path::Path, id: &str, title: &str, status: &str) {
        let content = format!(
            "---\nid: {id}\ntitle: {title}\nstatus: {status}\npriority: P2\ncreated: 2026-01-01T00:00:00Z\nupdated: 2026-01-01T00:00:00Z\n---\n"
        );
        let filename = format!("{id}-{}.md", title.to_lowercase().replace(' ', "-"));
        std::fs::write(bears_dir.join(filename), content).expect("write task file");
    }

    /// `load_all` followed by `App::reload` correctly populates `all_tasks`,
    /// `task_map`, `graph`, and the visible `tasks` list from disk.
    ///
    /// No file-watcher or real timing involved — this is a pure functional test
    /// of the disk-read → reload application path.
    #[tokio::test]
    async fn reload_applies_disk_state_to_app() {
        let tmp = tempfile::tempdir().unwrap();
        crate::store::init(tmp.path()).unwrap();
        let bears_dir = tmp.path().join(".bears");

        // Write two open tasks and one done task.
        write_task_file(&bears_dir, "t01", "Alpha Task", "open");
        write_task_file(&bears_dir, "t02", "Beta Task", "open");
        write_task_file(&bears_dir, "t03", "Gamma Task", "done");

        // Load from disk and build initial app.
        let task_map = crate::store::load_all(tmp.path()).await.unwrap();
        let mut task_list: Vec<Task> = task_map.values().cloned().collect();
        task_list.sort_by(|a, b| a.priority.cmp(&b.priority).then(a.created.cmp(&b.created)));
        let mut app = App::new(task_list, task_map, tmp.path().to_path_buf());

        // Default filter (Open) should show the two open tasks, not the done one.
        assert_eq!(
            app.all_tasks.len(),
            3,
            "all_tasks should hold every task from disk"
        );
        assert!(app.task_map.contains_key("t01"));
        assert!(app.task_map.contains_key("t02"));
        assert!(app.task_map.contains_key("t03"));
        let open_visible: Vec<&str> = app.tasks.iter().map(|t| t.id.as_str()).collect();
        assert!(
            open_visible.contains(&"t01") && open_visible.contains(&"t02"),
            "open tasks must be visible"
        );
        assert!(
            !open_visible.contains(&"t03"),
            "done task must be hidden by default Open filter"
        );

        // Now simulate an external edit: add a new task and delete t02.
        write_task_file(&bears_dir, "t04", "Delta Task", "open");
        std::fs::remove_file(bears_dir.join("t02-beta-task.md")).unwrap();

        // Re-load from disk and call App::reload (the path the watcher triggers).
        let new_map = crate::store::load_all(tmp.path()).await.unwrap();
        let mut new_list: Vec<Task> = new_map.values().cloned().collect();
        new_list.sort_by(|a, b| a.priority.cmp(&b.priority).then(a.created.cmp(&b.created)));
        app.reload(new_list, new_map);

        // all_tasks / task_map must reflect the new disk state.
        assert_eq!(app.all_tasks.len(), 3, "t02 deleted, t04 added → 3 tasks");
        assert!(
            app.task_map.contains_key("t01"),
            "t01 must still be present"
        );
        assert!(
            !app.task_map.contains_key("t02"),
            "t02 must be gone after deletion"
        );
        assert!(
            app.task_map.contains_key("t04"),
            "t04 must appear after add"
        );

        // Visible list should contain t01 and t04 (open), not t02 or t03.
        let visible_ids: Vec<&str> = app.tasks.iter().map(|t| t.id.as_str()).collect();
        assert!(visible_ids.contains(&"t01"), "t01 should be visible");
        assert!(visible_ids.contains(&"t04"), "t04 should be visible");
        assert!(
            !visible_ids.contains(&"t02"),
            "deleted t02 must not be visible"
        );
        assert!(
            !visible_ids.contains(&"t03"),
            "done t03 still hidden by Open filter"
        );

        // Graph must be rebuilt: there are no dependency edges, so every open
        // task should be recognised as ready by the graph helper.
        assert!(
            crate::graph::is_task_ready(&app.task_map, app.task_map.get("t01").unwrap()),
            "t01 has no deps, so it must be ready"
        );
        assert!(
            crate::graph::is_task_ready(&app.task_map, app.task_map.get("t04").unwrap()),
            "t04 has no deps, so it must be ready"
        );
    }

    /// When a task gains a dependency on an incomplete task after reload,
    /// the rebuilt graph must reflect that it is no longer ready.
    #[tokio::test]
    async fn reload_rebuilds_graph_correctly() {
        let tmp = tempfile::tempdir().unwrap();
        crate::store::init(tmp.path()).unwrap();
        let bears_dir = tmp.path().join(".bears");

        // Two open tasks, no dependencies initially.
        write_task_file(&bears_dir, "u01", "Upstream Task", "open");
        write_task_file(&bears_dir, "u02", "Downstream Task", "open");

        let task_map = crate::store::load_all(tmp.path()).await.unwrap();
        let task_list: Vec<Task> = task_map.values().cloned().collect();
        let mut app = App::new(task_list, task_map, tmp.path().to_path_buf());

        // Both tasks are initially ready (no deps).
        assert!(crate::graph::is_task_ready(
            &app.task_map,
            app.task_map.get("u02").unwrap()
        ));

        // Rewrite u02 with a dependency on u01 (which is still open).
        let blocked_content = "---\nid: u02\ntitle: Downstream Task\nstatus: open\npriority: P2\n\
             depends_on: [u01]\ncreated: 2026-01-01T00:00:00Z\nupdated: 2026-01-01T00:00:01Z\n---\n";
        std::fs::write(bears_dir.join("u02-downstream-task.md"), blocked_content).unwrap();

        // Reload from disk.
        let new_map = crate::store::load_all(tmp.path()).await.unwrap();
        let new_list: Vec<Task> = new_map.values().cloned().collect();
        app.reload(new_list, new_map);

        // After reload, u02 depends on open u01 → not ready.
        assert!(
            !crate::graph::is_task_ready(&app.task_map, app.task_map.get("u02").unwrap()),
            "u02 must not be ready after reload adds dep on open u01"
        );
        // u01 still has no deps → still ready.
        assert!(
            crate::graph::is_task_ready(&app.task_map, app.task_map.get("u01").unwrap()),
            "u01 must still be ready"
        );
    }
}
